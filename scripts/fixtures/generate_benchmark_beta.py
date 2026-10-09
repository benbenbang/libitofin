#!/usr/bin/env python3
"""Independent exact rational fixtures, without importing libitofin.

Cov = N/(N-1) * (sum(w*a*b)/W - mean(a)*mean(b)); benchmark
variance uses the same count correction. Zero-weight rows still count in N.
All input integers are exact; only the final CSV beta is rounded to float.
"""
from fractions import Fraction
from pathlib import Path

CASES = [
    ("unit", [1, 3, 2, 10], [1, 3, 2, 10], None),
    ("zero", [7, 7, 7, 7], [-1, 0, 2, 9], None),
    ("negative", [-1, -3, -2, -10], [1, 3, 2, 10], None),
    ("unweighted", [1, 3, 2, 10], [-1, 0, 2, 9], None),
    ("weighted", [1, 3, 2, 10], [-1, 0, 2, 9], [1, 2, 3, 0]),
    ("zero_weight_extreme", [1, 3, 2, 1000000], [-1, 0, 2, -999999], [1, 2, 3, 0]),
    ("two_rows", [3, 7], [1, 3], [1, 9]),
]
rows = ["name;asset;benchmark;weights;beta"]
for name, asset, benchmark, supplied in CASES:
    weights = supplied if supplied is not None else [1] * len(asset)
    total = sum(weights)
    mean_a = Fraction(sum(w * a for w, a in zip(weights, asset)), total)
    mean_b = Fraction(sum(w * b for w, b in zip(weights, benchmark)), total)
    correction = Fraction(len(asset), len(asset) - 1)
    covariance = correction * (Fraction(sum(w * a * b for w, a, b in zip(weights, asset, benchmark)), total) - mean_a * mean_b)
    variance = correction * (Fraction(sum(w * b * b for w, b in zip(weights, benchmark)), total) - mean_b**2)
    beta = covariance / variance
    serialize = lambda values: ",".join(map(str, values))
    rows.append(f"{name};{serialize(asset)};{serialize(benchmark)};{serialize(supplied or [])};{float(beta):.17g}")
root = Path(__file__).resolve().parents[2]
(root / "sdk/go/testdata/benchmark-beta.csv").write_text("\n".join(rows) + "\n")
