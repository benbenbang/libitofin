"""Weighted batch statistics and empirical loss measures."""

import math

import pytest

import itofin
from itofin import statistics


def test_weighted_mean_sample_variance_and_percentile():
    """Weighted center and dispersion retain the count-based sample correction."""
    observations = [1.0, 2.0, 3.0]
    weights = [1.0, 2.0, 1.0]

    assert statistics.mean(observations, weights=weights) == 2.0
    assert statistics.variance(observations, weights=weights) == 0.75
    assert statistics.standard_deviation(observations, weights=weights) == math.sqrt(0.75)
    assert statistics.percentile(observations, 0.5, weights=weights) == 2.0
    assert statistics.percentile(observations, 1.0, weights=weights) == 3.0


def test_signed_losses_and_weighted_empirical_risk():
    """Risk measures report loss magnitudes and respect empirical weights."""
    observations = [-10.0, -5.0, 1.0]
    weights = [1.0, 2.0, 17.0]
    expanded = [-10.0] + [-5.0] * 2 + [1.0] * 17

    assert statistics.value_at_risk(observations, 0.9, weights=weights) == 5.0
    assert statistics.expected_shortfall(observations, 0.9, weights=weights) == 10.0
    assert statistics.value_at_risk(expanded, 0.9) == 5.0
    assert statistics.expected_shortfall(expanded, 0.9) == 10.0
    assert statistics.value_at_risk([1.0, 2.0], 0.9) == 0.0

    shared = [-5.0, -2.0, 1.0, 2.0]
    shared_weights = [1.0, 10.0, 1.0, 8.0]
    assert statistics.value_at_risk(shared, 0.9, weights=shared_weights) == 2.0
    assert statistics.expected_shortfall(shared, 0.9, weights=shared_weights) == 5.0


@pytest.mark.parametrize(
    "observations,weights",
    [
        ([1.0, 2.0], [1.0]),
        ([float("nan")], None),
        ([float("inf")], None),
        ([1.0], [float("nan")]),
        ([1.0], [float("inf")]),
        ([1.0], [-1.0]),
        ([1.0], [0.0]),
    ],
)
def test_invalid_samples_and_weights(observations, weights):
    """Bad observations or weights fail through the core error bridge."""
    with pytest.raises(itofin.ItofinError):
        statistics.mean(observations, weights=weights)


def test_empty_one_sample_and_absent_tail_errors():
    """Insufficient input and empty ES tails are explicit errors."""
    with pytest.raises(itofin.ItofinError):
        statistics.mean([])
    with pytest.raises(itofin.ItofinError):
        statistics.variance([1.0])
    with pytest.raises(itofin.ItofinError):
        statistics.standard_deviation([1.0])
    with pytest.raises(itofin.ItofinError):
        statistics.percentile([], 0.5)
    with pytest.raises(itofin.ItofinError):
        statistics.value_at_risk([], 0.9)
    with pytest.raises(itofin.ItofinError):
        statistics.expected_shortfall([-10.0], 0.9)


@pytest.mark.parametrize("probability", [float("nan"), float("inf"), 0.0, -0.1, 1.1])
def test_invalid_percentile(probability):
    """A percentile must be finite and in (0, 1]."""
    with pytest.raises(itofin.ItofinError):
        statistics.percentile([1.0], probability)


@pytest.mark.parametrize("confidence", [float("nan"), float("inf"), 0.899, 1.0, -1.0])
@pytest.mark.parametrize("measure", [statistics.value_at_risk, statistics.expected_shortfall])
def test_invalid_risk_confidence(measure, confidence):
    """VaR and ES confidence must be finite and in [0.9, 1)."""
    with pytest.raises(itofin.ItofinError):
        measure([-10.0, 1.0], confidence)
