import argparse
import csv
import ctypes
import json
import math
from pathlib import Path
import re
import subprocess
import QuantLib

parser = argparse.ArgumentParser()
parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[3])
parser.add_argument("--output", type=Path, default=Path(__file__).resolve().parent)
parser.add_argument(
    "--bridge", type=Path, default=Path(__file__).with_name("merton76_oracle.dylib")
)
args = parser.parse_args()
HERE = args.output.resolve()
ROOT = args.repo.resolve()
HERE.mkdir(parents=True, exist_ok=True)
ctypes.CDLL(
    str(next(Path(QuantLib.__file__).parent.glob("_QuantLib*.so"))),
    mode=ctypes.RTLD_GLOBAL,
)
bridge = ctypes.CDLL(str(args.bridge.resolve()))
native = bridge.merton
native.argtypes = (
    [ctypes.c_int, ctypes.c_int]
    + [ctypes.c_double] * 8
    + [
        ctypes.c_int,
        ctypes.c_double,
        ctypes.c_ulong,
        ctypes.POINTER(ctypes.c_double),
        ctypes.c_char_p,
    ]
)
FIELDS = ["value", "delta", "gamma", "theta", "vega", "rho", "dividend_rho"]
BASE = dict(
    option_type="call",
    spot=100.0,
    strike=100.0,
    dividend_yield=0.02,
    risk_free_rate=0.05,
    volatility=0.2,
    jump_intensity=1.0,
    log_mean_jump=-0.1,
    log_jump_volatility=0.3,
    maturity_days=360,
    relative_accuracy=1e-12,
    max_iterations=4096,
    clock_variant=0,
)


def ql_result(c):
    a = (ctypes.c_double * 7)()
    err = ctypes.create_string_buffer(4096)
    rc = native(
        c["option_type"] == "call",
        c["clock_variant"],
        *[
            c[k]
            for k in [
                "spot",
                "strike",
                "dividend_yield",
                "risk_free_rate",
                "volatility",
                "jump_intensity",
                "log_mean_jump",
                "log_jump_volatility",
            ]
        ],
        c["maturity_days"],
        c["relative_accuracy"],
        c["max_iterations"],
        a,
        err,
    )
    if rc:
        return {"error": err.value.decode()}
    return dict(zip(FIELDS, a))


def normal(x):
    return 0.5 * math.erfc(-x / math.sqrt(2.0))


def black(s, k, q, r, v, t, call):
    sd = s * math.exp(-q * t)
    kd = k * math.exp(-r * t)
    std = abs(v) * math.sqrt(t)
    if std == 0:
        return max(sd - kd, 0) if call else max(kd - sd, 0)
    d1 = (math.log(sd / kd) + 0.5 * std * std) / std
    d2 = d1 - std
    return (
        sd * normal(d1) - kd * normal(d2)
        if call
        else kd * normal(-d2) - sd * normal(-d1)
    )


def independent(c, t=None):
    t = c["maturity_days"] / 360 if t is None else t
    s, k, q, r, sigma, lam, mu, dv = (
        c[k]
        for k in [
            "spot",
            "strike",
            "dividend_yield",
            "risk_free_rate",
            "volatility",
            "jump_intensity",
            "log_mean_jump",
            "log_jump_volatility",
        ]
    )
    mean = lam * t
    kap = math.expm1(mu + 0.5 * dv * dv)
    call = c["option_type"] == "call"
    if mean == 0:
        return black(s, k, q, r, sigma, t, call)
    nmax = math.ceil(mean + 14 * math.sqrt(mean) + 50)
    terms = []
    for n in range(nmax + 1):
        weight = math.exp(n * math.log(mean) - math.lgamma(n + 1) - mean)
        conditional_s = s * math.exp(-lam * kap * t + n * (mu + 0.5 * dv * dv))
        conditional_v = math.sqrt(sigma * sigma + n * dv * dv / t)
        terms.append(weight * black(conditional_s, k, q, r, conditional_v, t, call))
    return math.fsum(terms)


def clock_price(c):
    t = (c["maturity_days"] + 30) / 360
    u = c["maturity_days"] / 360
    divt = c["maturity_days"] / 365
    s, k, q, r, sigma, lam, mu, dv = (
        c[k]
        for k in [
            "spot",
            "strike",
            "dividend_yield",
            "risk_free_rate",
            "volatility",
            "jump_intensity",
            "log_mean_jump",
            "log_jump_volatility",
        ]
    )
    exponent = mu + 0.5 * dv * dv
    kap = math.expm1(exponent)
    mean = lam * math.exp(exponent) * t
    rr = r * (c["maturity_days"] / 365) / t
    out = []
    for n in range(math.ceil(mean + 14 * math.sqrt(mean) + 50) + 1):
        weight = math.exp(n * math.log(mean) - math.lgamma(n + 1) - mean)
        rn = rr - lam * kap + n * exponent / t
        vn = math.sqrt(sigma * sigma + n * dv * dv / t)
        out.append(
            weight * black(s, k, q * divt / u, rn, vn, u, c["option_type"] == "call")
        )
    return math.fsum(out)


def fd(c):
    result = {}
    for field, name, h in [
        ("spot", "delta", 0.01),
        ("risk_free_rate", "rho", 1e-4),
        ("dividend_yield", "dividend_rho", 1e-4),
        ("volatility", "vega", 1e-4),
    ]:
        values = {
            i: independent(dict(c, **{field: c[field] + i * h}))
            for i in [-2, -1, 0, 1, 2]
        }
        result[name] = (values[-2] - 8 * values[-1] + 8 * values[1] - values[2]) / (
            12 * h
        )
        if field == "spot":
            result["gamma"] = (
                -values[2]
                + 16 * values[1]
                - 30 * values[0]
                + 16 * values[-1]
                - values[-2]
            ) / (12 * h * h)
    t = c["maturity_days"] / 360
    h = 1e-4
    values = {i: independent(c, t + i * h) for i in [-2, -1, 1, 2]}
    result["theta"] = -(values[-2] - 8 * values[-1] + 8 * values[1] - values[2]) / (
        12 * h
    )
    return result


variants = [
    ("call", {}),
    ("put", dict(option_type="put")),
    ("positive_mean_call", dict(log_mean_jump=0.2, jump_intensity=2.5)),
    (
        "negative_mean_put",
        dict(
            option_type="put",
            log_mean_jump=-0.2,
            jump_intensity=5.0,
            log_jump_volatility=0.01,
            maturity_days=1800,
        ),
    ),
    ("zero_jump_call", dict(jump_intensity=0.0)),
    ("zero_jump_put", dict(jump_intensity=0.0, option_type="put")),
    (
        "deterministic_jump_call",
        dict(log_mean_jump=0.08, log_jump_volatility=0.0, jump_intensity=2.0),
    ),
    (
        "no_jump_size_call",
        dict(log_mean_jump=0.0, log_jump_volatility=0.0, jump_intensity=8.0),
    ),
    (
        "high_poisson_mean_call",
        dict(log_mean_jump=0.0, log_jump_volatility=0.02, jump_intensity=800.0),
    ),
    (
        "high_poisson_mean_put",
        dict(
            log_mean_jump=0.0,
            log_jump_volatility=0.02,
            jump_intensity=800.0,
            option_type="put",
        ),
    ),
    ("different_curve_clocks", dict(clock_variant=1)),
    (
        "negative_large_jump_put_tail",
        dict(
            option_type="put",
            strike=1.0,
            volatility=0.001,
            jump_intensity=10.0,
            log_mean_jump=-5.0,
            log_jump_volatility=0.0,
        ),
    ),
]
cases = []
for name, kw in variants:
    c = dict(BASE, **kw)
    expected = ql_result(c)
    assert "error" not in expected, (name, expected)
    price = clock_price(c) if c["clock_variant"] else independent(c)
    assert abs(price - expected["value"]) < 1e-8, (name, price, expected)
    numerical = None if c["clock_variant"] else fd(c)
    if numerical:
        for greek, v in numerical.items():
            assert abs(v - expected[greek]) < 1e-6, (name, greek, v, expected[greek])
    cases.append(
        dict(
            name=name,
            input=c,
            expected=expected,
            independent_price=price,
            finite_difference_greeks=numerical,
        )
    )
source = (ROOT / "QuantLib/test-suite/jumpdiffusion.cpp").read_text()
haug = []
for line in source.splitlines():
    match = re.search(r"\{ Option::(Call|Put),\s*(.*?)\s*\}", line)
    if not match or "Option::" in match[2]:
        continue
    nums = [float(x.strip()) for x in match[2].split(",")]
    if len(nums) != 10:
        continue
    strike, s, q, r, t, v, lam, gamma, value, tol = nums
    jv = v * math.sqrt(gamma / lam)
    sig = v * math.sqrt(1 - gamma)
    mu = -0.5 * jv * jv
    c = dict(
        BASE,
        option_type=match[1].lower(),
        spot=s,
        strike=strike,
        dividend_yield=q,
        risk_free_rate=r,
        volatility=sig,
        jump_intensity=lam,
        log_mean_jump=mu,
        log_jump_volatility=jv,
        maturity_days=round(t * 360),
    )
    got = ql_result(c)
    assert abs(got["value"] - value) <= tol
    haug.append(
        dict(
            input=c,
            total_volatility=v,
            gamma=gamma,
            expected=value,
            absolute_tolerance=tol,
            quantlib_value=got["value"],
        )
    )
failures = []
for name, kw in [
    ("insufficient_iterations", dict(max_iterations=1)),
    (
        "high_mean_small_budget",
        dict(
            jump_intensity=800.0,
            log_mean_jump=0.0,
            log_jump_volatility=0.02,
            max_iterations=100,
        ),
    ),
    ("zero_diffusion_with_jumps", dict(volatility=0.0)),
    ("fully_deterministic", dict(volatility=0.0, jump_intensity=0.0)),
]:
    c = dict(BASE, **kw)
    res = ql_result(c)
    failures.append(dict(name=name, input=c, result=res))
pinned = subprocess.check_output(
    ["git", "-C", str(ROOT / "QuantLib"), "rev-parse", "HEAD"], text=True
).strip()
provenance = dict(
    quantlib_version=QuantLib.__version__,
    pinned_quantlib_commit=pinned,
    reference_date="2026-10-02",
    default_daycount="Actual360",
    calendar="NullCalendar",
    compounding="Continuous",
    relative_accuracy=1e-12,
    max_iterations=4096,
    price_absolute_tolerance=1e-8,
    greek_absolute_tolerance=1e-8,
    finite_difference_absolute_tolerance=1e-6,
)
native_only = dict(
    provenance=provenance,
    cases=[
        {k: c[k] for k in ["name", "input", "expected", "independent_price"]}
        for c in cases
    ],
)
(HERE / "merton76-oracle.json").write_text(
    json.dumps(native_only, indent=2, sort_keys=True, allow_nan=False) + "\n"
)
keys = list(BASE) + [
    "total_volatility",
    "gamma",
    "expected",
    "absolute_tolerance",
    "quantlib_value",
]
with (HERE / "merton76-haug.csv").open("w", newline="") as f:
    writer = csv.DictWriter(f, fieldnames=keys, lineterminator="\n")
    writer.writeheader()
    writer.writerows(
        dict(c["input"], **{k: v for k, v in c.items() if k != "input"}) for c in haug
    )
zero_vol = []
for name, kw in [
    ("zero_diffusion_with_jumps", dict(volatility=0.0)),
    ("fully_deterministic", dict(volatility=0.0, jump_intensity=0.0)),
    (
        "zero_prefix_before_nonzero_jump_payoffs",
        dict(
            strike=1000.0,
            volatility=0.001,
            jump_intensity=0.01,
            log_mean_jump=1.0,
            log_jump_volatility=0.0,
        ),
    ),
]:
    c = dict(BASE, **kw)
    zero_vol.append(
        dict(
            name=name,
            input=c,
            expected=dict(value=independent(c), **fd(c)),
            source="Independent original-intensity price mixture plus five-point derivatives",
            absolute_tolerance=1e-6 if c["volatility"] == 0 else 1e-10,
        )
    )
for edge in failures:
    edge["result"] = {
        k: ("NaN" if isinstance(v, float) and math.isnan(v) else v)
        for k, v in edge["result"].items()
    }
(HERE / "merton76-edge-cases.json").write_text(
    json.dumps(
        dict(cases=zero_vol, upstream_edge_behavior=failures),
        indent=2,
        sort_keys=True,
        allow_nan=False,
    )
    + "\n"
)
print(
    f"Generated {len(cases)} native cases, {len(haug)} Haug rows, {len(zero_vol)} edge cases"
)
