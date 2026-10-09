"""Beta for caller-aligned return samples; no benchmark feed is fetched."""

# standard library
from math import isclose

# itofin library
from itofin import statistics

asset = [-0.01, 0.02, 0.05, 0.03]
benchmark = [-0.02, 0.0, 0.01, 0.04]
beta = statistics.benchmark_beta(asset, benchmark, weights=[1, 2, 3, 0])
assert isclose(beta, 84 / 41, rel_tol=0, abs_tol=2e-12)
print(f"Benchmark beta: {beta:.8f}")
