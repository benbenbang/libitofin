"""Generate native pinned QuantLib variance-swap replication fixtures."""

import argparse
from datetime import date, timedelta
import hashlib
import json
import math
from pathlib import Path
import subprocess

from build_native import build

BASE = dict(
    evaluation_date="2026-10-05",
    start_date="2026-10-05",
    maturity_date="2027-10-05",
    day_counter="Actual365Fixed",
    maturity_days=365,
    spot=100.0,
    risk_free_rate=0.03,
    dividend_yield=0.0,
    volatility=0.20,
    variance_strike=0.04,
    notional=50000.0,
    position="long",
    dk=5.0,
    call_strikes=[100.0, 110.0, 120.0, 130.0, 140.0, 150.0],
    put_strikes=[50.0, 60.0, 70.0, 80.0, 90.0, 100.0],
)
MARKET = ["spot", "risk_free_rate", "dividend_yield", "volatility"]


def write(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n")


def encode(case, updates):
    smile = list(zip(case.get("smile_strikes", []), case.get("smile_vols", [])))
    header = [
        *[case[key] for key in MARKET],
        case["variance_strike"],
        case["notional"],
        1 if case["position"] == "long" else -1,
        case["maturity_days"],
        case["dk"],
        len(case["call_strikes"]),
        len(case["put_strikes"]),
        len(smile),
    ]
    lines = [
        " ".join(map(str, header)),
        " ".join(map(str, case["call_strikes"])),
        " ".join(map(str, case["put_strikes"])),
    ]
    lines.extend(f"{strike} {vol}" for strike, vol in smile)
    lines.append(str(len(updates)))
    lines.extend(
        " ".join([update["name"], *[str(update[key]) for key in MARKET]])
        for update in updates
    )
    return "\n".join(lines) + "\n"


def native(binary, case, updates=()):
    return json.loads(
        subprocess.check_output([str(binary)], input=encode(case, updates), text=True)
    )


def black(case, strike, call):
    t = case["maturity_days"] / 365.0
    sigma = case["volatility"]
    if "smile_strikes" in case:
        sigma = case["smile_vols"][case["smile_strikes"].index(strike)]
    spot = case["spot"] * math.exp(-case["dividend_yield"] * t)
    exercise = strike * math.exp(-case["risk_free_rate"] * t)
    std = sigma * math.sqrt(t)
    if std == 0:
        return max(spot - exercise, 0) if call else max(exercise - spot, 0)
    d1 = (math.log(spot / exercise) + std * std / 2) / std
    d2 = d1 - std

    def normal(x):
        return math.erfc(-x / math.sqrt(2)) / 2

    return (
        spot * normal(d1) - exercise * normal(d2)
        if call
        else exercise * normal(-d2) - spot * normal(-d1)
    )


def independent(case):
    t = case["maturity_days"] / 365.0
    f = min(case["call_strikes"])

    def payoff(strike):
        return 2 / t * ((strike - f) / f - math.log(strike / f))

    weights = []
    for call, raw in [(True, case["call_strikes"]), (False, case["put_strikes"])]:
        strikes = sorted(set(raw), reverse=not call)
        strikes.append(strikes[-1] + case["dk"] if call else strikes[-1] - case["dk"])
        previous = 0.0
        for left, right in zip(strikes, strikes[1:]):
            slope = abs((payoff(right) - payoff(left)) / (right - left))
            weights.append(
                dict(
                    option_type="call" if call else "put",
                    strike=left,
                    weight=slope - previous,
                    option_price=black(case, left, call),
                )
            )
            previous = slope
    discount = math.exp(-case["risk_free_rate"] * t)
    option_cost = math.fsum(row["weight"] * row["option_price"] for row in weights)
    s = case["spot"]
    variance = (
        2 * case["risk_free_rate"]
        - 2 / t * ((s / discount - f) / f + math.log(f / s))
        + option_cost / discount
    )
    npv = (
        (1 if case["position"] == "long" else -1)
        * discount
        * case["notional"]
        * (variance - case["variance_strike"])
    )
    return dict(variance=variance, npv=npv, weights=weights)


def verify(case, result):
    expected = independent(case)
    for key, absolute in [("variance", 2e-14), ("npv", 2e-9)]:
        if not math.isfinite(result[key]) or not math.isclose(
            result[key], expected[key], rel_tol=3e-12, abs_tol=absolute
        ):
            raise ValueError(
                f"Independent {key} mismatch: {result[key]} != {expected[key]}"
            )
    if len(result["weights"]) != len(expected["weights"]):
        raise ValueError("Native weight count mismatch")
    for actual, wanted in zip(result["weights"], expected["weights"]):
        if (
            actual["option_type"] != wanted["option_type"]
            or actual["strike"] != wanted["strike"]
        ):
            raise ValueError("Native weight ordering mismatch")
        for key in ["weight", "option_price"]:
            if not math.isfinite(actual[key]) or not math.isclose(
                actual[key], wanted[key], rel_tol=3e-12, abs_tol=2e-14
            ):
                raise ValueError(f"Native {key} mismatch")


def cases():
    smile_strikes = list(range(50, 140, 5))
    smile_vols = [(40 - i) / 100 for i in range(10, 28)]
    return [
        (
            "literature_smile",
            dict(
                risk_free_rate=0.05,
                maturity_days=90,
                call_strikes=list(range(100, 140, 5)),
                put_strikes=list(range(50, 105, 5)),
                smile_strikes=smile_strikes,
                smile_vols=smile_vols,
            ),
        ),
        ("flat_base", {}),
        ("short", dict(position="short")),
        ("strike_bump", dict(variance_strike=0.06)),
        ("notional_double", dict(notional=100000.0)),
        ("dividend_positive", dict(dividend_yield=0.02)),
        ("dividend_negative", dict(dividend_yield=-0.01)),
        ("negative_rate", dict(risk_free_rate=-0.01)),
        (
            "boundary_90",
            dict(
                call_strikes=list(range(90, 151, 10)),
                put_strikes=list(range(50, 91, 10)),
            ),
        ),
        (
            "boundary_110",
            dict(
                call_strikes=list(range(110, 151, 10)),
                put_strikes=list(range(50, 111, 10)),
            ),
        ),
        (
            "unsorted_duplicate",
            dict(
                call_strikes=[150, 100, 120, 100, 140, 110, 130],
                put_strikes=[80, 100, 50, 70, 100, 60, 90],
            ),
        ),
        ("short_maturity", dict(maturity_days=7)),
        ("zero_volatility", dict(risk_free_rate=0.0, volatility=0.0)),
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--quantlib", type=Path, required=True)
    parser.add_argument(
        "--boost-include", type=Path, default=Path("/opt/homebrew/include")
    )
    parser.add_argument("--cxx", default="clang++")
    parser.add_argument("--build-dir", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, default=Path(__file__).parent)
    args = parser.parse_args()
    binary, provenance = build(args)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    filenames, results = [], {}
    for name, changes in cases():
        inputs = dict(BASE, **changes)
        inputs["maturity_date"] = str(
            date.fromisoformat(inputs["evaluation_date"])
            + timedelta(days=inputs["maturity_days"])
        )
        result = native(binary, inputs)["initial"]
        verify(inputs, result)
        results[name] = result
        filename = name + ".json"
        filenames.append(filename)
        write(args.output_dir / filename, dict(name=name, inputs=inputs, native=result))
    equal = dict(BASE, variance_strike=results["flat_base"]["variance"])
    result = native(binary, equal)["initial"]
    verify(equal, result)
    if result["npv"] != 0:
        raise ValueError("Equal native variance strike must give exact zero NPV")
    filenames.append("equal_variance_strike.json")
    write(
        args.output_dir / filenames[-1],
        dict(name="equal_variance_strike", inputs=equal, native=result),
    )
    updates = [
        dict(name=name, **{key: changes.get(key, BASE[key]) for key in MARKET})
        for name, changes in [
            ("spot", dict(spot=105.0)),
            ("rate", dict(risk_free_rate=0.04)),
            ("dividend", dict(dividend_yield=0.02)),
            ("volatility", dict(volatility=0.25)),
            (
                "combined",
                dict(
                    spot=105.0,
                    risk_free_rate=0.04,
                    dividend_yield=0.02,
                    volatility=0.25,
                ),
            ),
        ]
    ]
    update_result = native(binary, BASE, updates)
    previous = update_result["initial"]
    for actual, market in zip(update_result["updates"], updates):
        verify(
            dict(BASE, **{key: market[key] for key in MARKET}), actual["recalculated"]
        )
        if (
            actual["cached_variance"] != previous["variance"]
            or actual["cached_npv"] != previous["npv"]
        ):
            raise ValueError("Pinned engine cache omission changed")
        actual["market"] = {key: market[key] for key in MARKET}
        previous = actual["recalculated"]
        del actual["recalculated"]["weights"]
    filenames.append("live_updates.json")
    write(
        args.output_dir / filenames[-1],
        dict(
            name="live_updates",
            inputs=BASE,
            native=update_result["initial"],
            updates=update_result["updates"],
        ),
    )
    refinements = []
    for step in [5.0, 2.5, 1.25]:
        inputs = dict(
            BASE,
            risk_free_rate=0.0,
            dk=step,
            call_strikes=[100 + i * step for i in range(int(200 / step) + 1)],
            put_strikes=[5 + i * step for i in range(int(95 / step) + 1)],
        )
        if inputs["put_strikes"][0] <= step:
            inputs["put_strikes"] = inputs["put_strikes"][1:]
        result = native(binary, inputs)["initial"]
        verify(inputs, result)
        refinements.append(
            dict(
                step=step,
                min_put=min(inputs["put_strikes"]),
                max_call=300.0,
                boundary=100.0,
                native_variance=result["variance"],
                native_npv=result["npv"],
                weight_count=len(result["weights"]),
                sigma_squared=0.04,
                absolute_error=abs(result["variance"] - 0.04),
                input_sha256=hashlib.sha256(encode(inputs, ()).encode()).hexdigest(),
            )
        )
    if not all(
        right["absolute_error"] < left["absolute_error"]
        for left, right in zip(refinements, refinements[1:])
    ):
        raise ValueError("Flat-vol strip refinement must improve observed error")
    write(
        args.output_dir / "oracle.json",
        dict(
            cases=filenames,
            provenance=provenance,
            refinements=refinements,
            tolerances=dict(
                variance_absolute=2e-14,
                npv_absolute=2e-9,
                weight_absolute=2e-14,
                relative=3e-12,
            ),
        ),
    )
    print(
        f"Generated {len(filenames)} independent native cases and {len(refinements)} refinements"
    )


if __name__ == "__main__":
    main()
