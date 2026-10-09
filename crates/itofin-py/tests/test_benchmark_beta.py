"""Exact-rational benchmark beta fixtures and stateless error parity."""

import csv
import math
from pathlib import Path

import pytest

import itofin
from itofin import statistics


FIXTURES = Path(__file__).resolve().parents[3] / "sdk/go/testdata/benchmark-beta.csv"


def test_benchmark_beta_independent_fixtures():
    with FIXTURES.open() as source:
        for case in csv.DictReader(source, delimiter=";"):
            asset = [float(v) for v in case["asset"].split(",")]
            benchmark = [float(v) for v in case["benchmark"].split(",")]
            weights = [float(v) for v in case["weights"].split(",")] if case["weights"] else None
            original = asset.copy(), benchmark.copy(), None if weights is None else weights.copy()
            for _ in range(10):
                actual = statistics.benchmark_beta(asset, benchmark, weights=weights)
                assert actual == pytest.approx(float(case["beta"]), abs=2e-12, rel=0)
            assert (asset, benchmark, weights) == original
    assert statistics.benchmark_beta([-0.01, 0.02, 0.05, 0.03], [-0.02, 0, 0.01, 0.04], weights=[1, 2, 3, 0]) == pytest.approx(84 / 41, abs=2e-12, rel=0)


@pytest.mark.parametrize("asset,benchmark,weights", [
    ([], [], None), ([1], [2], None), ([1, 2], [1], None),
    ([1, 2], [3, 3], None), ([1, 2], [1, 2], []),
    ([1, 2], [1, 2], [1]), ([1, 2], [1, 2], [0, 0]),
    ([1, 2], [1, 2], [-1, 2]), ([1, 2], [1, 2], [math.nan, 1]),
    ([1, 2], [1, 2], [math.inf, 1]), ([1, 2], [1, 2], [1e308, 1e308]),
    ([math.nan, 2], [1, 2], [0, 1]), ([1, 2], [math.inf, 2], None),
    ([1e308, -1e308], [1, 2], None), ([1, 2], [1e-200, 2e-200], None),
    ([0] * 100001, [0] * 100001, None),
])
def test_benchmark_beta_errors(asset, benchmark, weights):
    with pytest.raises(itofin.ItofinError):
        statistics.benchmark_beta(asset, benchmark, weights=weights)
