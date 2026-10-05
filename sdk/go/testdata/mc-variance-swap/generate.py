"""Generate actual pinned QuantLib MC variance fixtures and independent replay."""

import argparse
import json
import math
import subprocess
from datetime import date, timedelta
from pathlib import Path

from build_native import build, digest
from replay_external import value as replay_external

BASE = {
    "evaluation_date": "2026-10-06",
    "start_date": "2026-10-06",
    "maturity_days": 90,
    "spot": 100.0,
    "risk_free_rate": 0.05,
    "dividend_yield": 0.0,
    "volatility": 0.2,
    "variance_strike": 0.04,
    "notional": 50000.0,
    "position": "long",
    "volatility_kind": "constant",
    "steps": 0,
    "steps_per_year": 250,
    "samples": 1023,
    "tolerance": 0.0,
    "max_samples": 0,
    "seed": 42,
}
MARKET = ["spot", "risk_free_rate", "dividend_yield", "volatility"]


def close(actual, expected, absolute):
    return math.isfinite(actual) and abs(actual - expected) <= max(
        absolute, 3e-12 * abs(expected)
    )


def write(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n")


def encode(case, updates):
    fields = [
        {"constant": 0, "variance_curve": 1, "external_local_vol": 2}[
            case["volatility_kind"]
        ],
        *[case[key] for key in MARKET],
        case["variance_strike"],
        case["notional"],
        1 if case["position"] == "long" else -1,
        case["maturity_days"],
        case["steps"],
        case["steps_per_year"],
        case["samples"],
        case["tolerance"],
        case["max_samples"],
        case["seed"],
    ]
    lines = [" ".join(map(str, fields)), str(len(updates))]
    lines.extend(" ".join(str(update[key]) for key in MARKET) for update in updates)
    return "\n".join(lines) + "\n"


def black_variance(t):
    first, last = 36 / 365, 90 / 365
    v1, v2 = first * 0.1 * 0.1, last * 0.2 * 0.2
    if t <= first:
        return t * 0.1 * 0.1
    if t <= last:
        return v1 + (v2 - v1) * (t - first) / (last - first)
    return v2 * t / last


def independent(case):
    maturity = case["maturity_days"] / 365
    steps = case["steps"] or max(int(case["steps_per_year"] * maturity), 1)
    grid_dt = maturity / steps
    end = grid_dt * steps
    intervals = int(end / grid_dt)
    spacing = end / intervals

    def integrand(t):
        if case["volatility_kind"] == "constant":
            return case["volatility"] * case["volatility"]
        increment = 1 / 365
        slope = (black_variance(t + increment) - black_variance(t)) / increment
        local_vol = math.sqrt(slope)
        return local_vol * local_vol

    accumulator = 0.5 * (integrand(0) + integrand(end))
    t = spacing
    for _ in range(1, intervals):
        accumulator += integrand(t)
        t += spacing
    variance = accumulator * spacing / end
    discount = math.exp(-case["risk_free_rate"] * maturity)
    npv = (
        (1 if case["position"] == "long" else -1)
        * discount
        * case["notional"]
        * (variance - case["variance_strike"])
    )
    return {
        "variance": variance,
        "npv": npv,
        "path_steps": steps,
        "integration_intervals": intervals,
    }


def verify(case, actual):
    wanted = independent(case)
    for key, absolute in [("variance", 2e-14), ("npv", 2e-9)]:
        if not math.isfinite(actual[key]) or not close(
            actual[key], wanted[key], absolute
        ):
            raise ValueError(
                f"Independent {key} mismatch {actual[key]} != {wanted[key]}"
            )
    for key in ["path_steps", "integration_intervals"]:
        if actual[key] != wanted[key]:
            raise ValueError(f"Native {key} mismatch")
    samples = case["samples"] or 1023
    if actual["samples"] != samples:
        raise ValueError("Native sample count mismatch")
    if not 0 <= actual["variance_error"] <= 2e-14:
        raise ValueError(
            "Time-only local variance must have roundoff-only sampling error"
        )
    multiplier = (
        (1 if case["position"] == "long" else -1)
        * math.exp(-case["risk_free_rate"] * case["maturity_days"] / 365)
        * case["notional"]
    )
    expected_error = multiplier * actual["variance_error"]
    if not close(actual["error_estimate"], expected_error, 2e-9):
        raise ValueError("Native signed monetary error mismatch")


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
    recipes = [
        ("literature_curve", {"volatility_kind": "variance_curve"}),
        ("flat_base", {}),
        ("short", {"position": "short", "variance_strike": 0.05}),
        ("zero_volatility", {"volatility": 0.0}),
        ("negative_rate_dividend", {"risk_free_rate": -0.01, "dividend_yield": 0.02}),
        ("explicit_steps", {"steps": 7, "steps_per_year": 0, "maturity_days": 3}),
        ("minimum_grid", {"steps_per_year": 1, "maturity_days": 3, "samples": 2}),
        ("tolerance", {"samples": 0, "tolerance": 1e-8, "max_samples": 2046}),
        (
            "curve_tolerance",
            {
                "volatility_kind": "variance_curve",
                "samples": 0,
                "tolerance": 1e-8,
                "max_samples": 2046,
            },
        ),
    ]
    cases = []
    for name, changes in recipes:
        inputs = dict(BASE, **changes)
        inputs["maturity_date"] = str(
            date.fromisoformat(inputs["evaluation_date"])
            + timedelta(days=inputs["maturity_days"])
        )
        if inputs["volatility_kind"] == "variance_curve":
            inputs["curve_days"] = [36, 90]
            inputs["curve_vols"] = [0.1, 0.2]
        actual = json.loads(
            subprocess.check_output([str(binary)], input=encode(inputs, []), text=True)
        )["initial"]
        verify(inputs, actual)
        cases.append({"name": name, "inputs": inputs, "native": actual})
    literature = cases[0]["native"]["variance"]
    if abs(literature - 0.04) > 3e-4:
        raise ValueError("Original native literature tolerance exceeded")
    write(args.output_dir / "cases.json", {"cases": cases})
    updates = [
        dict(BASE, **changes)
        for changes in [
            {"spot": 105},
            {"risk_free_rate": 0.04},
            {"dividend_yield": 0.02},
            {"volatility": 0.25},
        ]
    ]
    live = json.loads(
        subprocess.check_output([str(binary)], input=encode(BASE, updates), text=True)
    )
    verify(BASE, live["initial"])
    records = []
    for market, actual in zip(updates, live["updates"]):
        verify(market, actual)
        records.append(
            {"market": {key: market[key] for key in MARKET}, "native": actual}
        )
    write(
        args.output_dir / "live_updates.json",
        {"inputs": BASE, "native": live["initial"], "updates": records},
    )
    external = []
    for name, changes in [
        ("external_fixed", {"samples": 1023}),
        ("external_short", {"samples": 1023, "position": "short"}),
        ("external_tolerance", {"samples": 0, "tolerance": 1e-5, "max_samples": 16368}),
    ]:
        inputs = dict(
            BASE,
            volatility_kind="external_local_vol",
            steps=7,
            steps_per_year=0,
            **changes,
        )
        actual = json.loads(
            subprocess.check_output([str(binary)], input=encode(inputs, []), text=True)
        )["initial"]
        count = inputs["samples"] or 1023
        while True:
            replay = replay_external(inputs, count)
            if inputs["samples"] or replay["variance_error"] <= inputs["tolerance"]:
                break
            error, tolerance = replay["variance_error"], inputs["tolerance"]
            batch = int(
                max(count * (error * error) / tolerance / tolerance * 0.8 - count, 1023)
            )
            count += min(batch, inputs["max_samples"] - count)
            if count >= inputs["max_samples"]:
                raise ValueError("Independent tolerance budget exhausted")
        if actual["samples"] != count:
            raise ValueError("Independent adaptive sample count mismatch")
        for key, absolute in [
            ("variance", 2e-14),
            ("npv", 2e-9),
            ("variance_error", 2e-14),
            ("error_estimate", 2e-9),
        ]:
            if not close(actual[key], replay[key], absolute):
                raise ValueError(f"Independent stochastic {key} mismatch")
        external.append({"name": name, "inputs": inputs, "native": actual})
    write(args.output_dir / "external_cases.json", {"cases": external})
    provenance["independent_replay_sha256"] = digest(
        Path(__file__).with_name("replay_external.py")
    )
    write(
        args.output_dir / "oracle.json",
        {
            "provenance": provenance,
            "tolerances": {
                "relative": 3e-12,
                "variance_absolute": 2e-14,
                "npv_absolute": 2e-9,
                "variance_error_absolute": 2e-14,
                "error_estimate_absolute": 2e-9,
                "literature_absolute": 3e-4,
            },
        },
    )
    print(f"Generated {len(cases)} native MC cases and {len(records)} live updates")


if __name__ == "__main__":
    main()
