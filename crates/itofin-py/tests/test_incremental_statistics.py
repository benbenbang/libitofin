"""Bounded-memory statistics facade and weighted moment parity."""

import math

import pytest

from itofin import ItofinError, statistics


def test_incremental_weighted_moments_and_reset():
    acc = statistics.IncrementalStatistics()
    acc.add_batch([-4, -2, 2, 8], weights=[1, 2, 1, 0])
    assert acc.samples() == 4
    assert acc.downside_samples() == 2
    assert acc.weight_sum() == 4
    assert acc.downside_weight_sum() == 3
    assert acc.min() == -4
    assert acc.max() == 8
    assert acc.mean() == pytest.approx(-1.5)
    assert acc.variance() == pytest.approx(19 / 3)
    assert acc.standard_deviation() == pytest.approx(math.sqrt(19 / 3))
    assert acc.error_estimate() == pytest.approx(math.sqrt(19 / 12))
    assert acc.downside_variance() == 16
    assert acc.downside_deviation() == 4
    assert acc.mean() == pytest.approx(statistics.mean([-4, -2, 2, 8], weights=[1, 2, 1, 0]))
    assert acc.variance() == pytest.approx(
        statistics.variance([-4, -2, 2, 8], weights=[1, 2, 1, 0])
    )
    with pytest.raises(ItofinError):
        acc.add_batch([1, math.nan])
    assert acc.samples() == 4
    with pytest.raises(ItofinError):
        acc.add_batch([1, 2], weights=[1])
    assert acc.samples() == 4
    acc.reset()
    assert acc.samples() == 0
    assert acc.weight_sum() == 0
    with pytest.raises(ItofinError):
        acc.mean()
    with pytest.raises(ItofinError):
        acc.add(1e100)
    with pytest.raises(ItofinError):
        acc.add_batch([1, 1e100])
    assert acc.samples() == 0
    acc.add_batch([1, 2, 3, 4])
    assert acc.skewness() == pytest.approx(0)
    assert acc.kurtosis() == pytest.approx(-1.2)
    assert not hasattr(acc, "percentile")
    assert not hasattr(acc, "value_at_risk")


def test_incremental_large_offset_and_zero_weight():
    acc = statistics.IncrementalStatistics()
    acc.add(-20, weight=0)
    assert acc.samples() == 1
    assert acc.min() == -20
    with pytest.raises(ItofinError):
        acc.mean()
    for offset in range(4):
        acc.add(1e12 + offset)
    assert acc.mean() == 1e12 + 1.5
    assert acc.variance() == pytest.approx(1.5625)
    assert acc.downside_samples() == 1
    with pytest.raises(ItofinError):
        acc.downside_variance()
    with pytest.raises(ItofinError):
        acc.add(math.inf)
    assert acc.samples() == 5
