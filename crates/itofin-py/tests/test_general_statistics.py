"""Stored-sample GeneralStatistics binding behavior."""

import math

import pytest

from itofin import statistics


def test_weighted_queries_add_after_percentile_and_reset():
    stats = statistics.GeneralStatistics()
    stats.add_batch([-10.0, -5.0, 1.0], weights=[1.0, 2.0, 17.0])
    assert stats.samples() == 3
    assert stats.weight_sum() == 20.0
    assert stats.mean() == pytest.approx(-0.15)
    assert stats.variance() == pytest.approx(12.49125)
    assert stats.value_at_risk(0.9) == 5.0
    assert stats.expected_shortfall(0.9) == 10.0
    assert stats.percentile(0.5) == 1.0
    with pytest.raises((ValueError, RuntimeError)):
        stats.add_batch([2.0, math.nan])
    assert stats.samples() == 3
    stats.add(-20.0)
    assert stats.percentile(0.01) == -20.0
    stats.reset()
    assert stats.samples() == 0
    assert stats.weight_sum() == 0.0
    with pytest.raises(Exception):
        stats.mean()


def test_zero_weight_counts_and_invalid_mutation_is_atomic():
    stats = statistics.GeneralStatistics()
    stats.add(1.0, 0.0)
    assert stats.samples() == 1
    assert stats.min() == 1.0
    assert stats.max() == 1.0
    with pytest.raises(Exception):
        stats.mean()
    stats.add_batch([3.0, 5.0])
    assert stats.mean() == 4.0
    for probability in (math.nan, math.inf, -math.inf):
        with pytest.raises(ValueError):
            stats.percentile(probability)
        with pytest.raises(ValueError):
            stats.top_percentile(probability)
    for value, weight in [(math.nan, 1.0), (1.0, -1.0), (1.0, math.inf)]:
        with pytest.raises(ValueError):
            stats.add(value, weight)
    assert stats.samples() == 3


def test_zero_weight_conditional_tails_raise_explicit_errors():
    stats = statistics.GeneralStatistics()
    stats.add_batch([-2.0, -1.0, 1.0], weights=[0.0, 0.0, 1.0])
    for query in (
        stats.semi_variance,
        stats.semi_deviation,
        stats.downside_variance,
        stats.downside_deviation,
        lambda: stats.regret(0.0),
        lambda: stats.average_shortfall(0.0),
    ):
        with pytest.raises(ValueError, match="positive-weight"):
            query()
