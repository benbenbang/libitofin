"""Compile pinned native QuantLib and replay standalone Euler GBM fixtures."""

import argparse
import csv
import json
import math
import subprocess
from pathlib import Path

from build_native import build

FIELDS = ["initial_value", "mu", "volatility", "time", "state", "dt", "dw"]


def close(actual, expected, absolute):
    return math.isfinite(actual) and abs(actual - expected) <= max(
        absolute, 3e-12 * abs(expected)
    )


def write(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n")


def independent(case):
    x, mu, sigma, dt, dw = [
        case[key] for key in ["state", "mu", "volatility", "dt", "dw"]
    ]
    drift, diffusion = mu * x, sigma * x
    expectation = x + drift * dt
    std = diffusion * math.sqrt(dt)
    return {
        "x0": case["initial_value"],
        "drift": drift,
        "diffusion": diffusion,
        "expectation": expectation,
        "variance": diffusion * diffusion * dt,
        "std_deviation": std,
        "evolve": expectation + std * dw,
    }


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
        ("positive", [100, 0.05, 0.2, 0, 100, 0.25, 0.5]),
        ("negative_state", [-100, 0.05, 0.2, 1, -80, 0.5, -0.75]),
        ("zero_state", [0, -0.03, 0.2, 2, 0, 1, 1]),
        ("negative_drift", [12, -0.3, 0.45, 0.75, 15, 0.1, -1.5]),
        ("zero_volatility", [-20, 0.2, 0, 0, -10, 0.5, 2]),
        ("zero_dt", [100, 0.1, 0.4, 5, 90, 0, -3]),
        ("cross_zero", [100, 0.1, 0.8, 0, 100, 1, -2]),
        ("fractional", [0.123, -0.007, 0.031, 0.37, 0.234, 0.019, -0.127]),
        ("zero_initial_positive_state", [0, 0.03, 0.2, 0, 30, 2, 0.1]),
    ]
    cases = []
    for name, values in recipes:
        inputs = dict(zip(FIELDS, values))
        actual = json.loads(
            subprocess.check_output(
                [str(binary)], input=" ".join(map(str, values)), text=True
            )
        )
        expected = independent(inputs)
        for key, value in actual.items():
            if not math.isfinite(value) or not close(value, expected[key], 2e-14):
                raise ValueError(f"Independent {name}/{key} mismatch")
        cases.append({"name": name, "inputs": inputs, "native": actual})
    write(args.output_dir / "cases.json", {"cases": cases})
    results = [
        "x0",
        "drift",
        "diffusion",
        "expectation",
        "variance",
        "std_deviation",
        "evolve",
    ]
    with (args.output_dir / "cases.csv").open("w", newline="") as stream:
        writer = csv.writer(stream, lineterminator="\n")
        writer.writerow(["name", *FIELDS, *results])
        for case in cases:
            writer.writerow(
                [
                    case["name"],
                    *[case["inputs"][key] for key in FIELDS],
                    *[case["native"][key] for key in results],
                ]
            )
    write(
        args.output_dir / "oracle.json",
        {
            "provenance": provenance,
            "tolerances": {"relative": 3e-12, "scalar_absolute": 2e-14},
        },
    )
    print(f"Generated {len(cases)} native Euler GBM cases")


if __name__ == "__main__":
    main()
