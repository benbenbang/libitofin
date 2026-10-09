"""Independent all-N downside and sample-Sharpe performance ratio contracts."""

import json
import math
from pathlib import Path

import pytest

import itofin
from itofin import statistics

FIXTURE = Path(__file__).resolve().parents[3] / "scripts/fixtures/performance-ratios/oracle.json"


def test_performance_ratios_decimal_oracle():
    """Each binding result matches independent Decimal definitions without mutation."""
    cases = json.loads(FIXTURE.read_text())["cases"]
    for case in cases:
        returns = case["returns"]
        before = returns.copy()
        assert statistics.target_downside_deviation(returns, case["target"]) == pytest.approx(
            case["downside"], rel=1e-13, abs=1e-14
        )
        assert statistics.sharpe_ratio(
            returns, case["target"], periods_per_year=case["periods_per_year"]
        ) == pytest.approx(case["sharpe"], rel=1e-13, abs=1e-14)
        assert statistics.sortino_ratio(
            returns, case["target"], periods_per_year=case["periods_per_year"]
        ) == pytest.approx(case["sortino"], rel=1e-13, abs=1e-14)
        assert returns == before


@pytest.mark.parametrize("returns", [[], [1.0], [1.0, math.nan], [1.0, math.inf]])
def test_performance_ratios_invalid_samples(returns):
    """Sample validation is shared with Rust, not Python-specific defaults."""
    with pytest.raises(itofin.ItofinError):
        statistics.sharpe_ratio(returns, 0.0, periods_per_year=12.0)
    with pytest.raises(itofin.ItofinError):
        statistics.sortino_ratio(returns, 0.0, periods_per_year=12.0)


@pytest.mark.parametrize("frequency", [0.0, -1.0, math.nan, math.inf])
def test_performance_ratios_invalid_frequency(frequency):
    """Frequency is explicitly finite-positive for both ratios."""
    with pytest.raises(itofin.ItofinError):
        statistics.sharpe_ratio([-1.0, 2.0], 0.0, periods_per_year=frequency)
    with pytest.raises(itofin.ItofinError):
        statistics.sortino_ratio([-1.0, 2.0], 0.0, periods_per_year=frequency)


@pytest.mark.parametrize("target", [math.nan, math.inf, -math.inf])
def test_performance_ratios_invalid_targets(target):
    """Targets are scalar per-period values, never inferred or silently broadcast."""
    with pytest.raises(itofin.ItofinError):
        statistics.target_downside_deviation([-1.0, 2.0], target)
    with pytest.raises(itofin.ItofinError):
        statistics.sharpe_ratio([-1.0, 2.0], target, periods_per_year=12.0)
    with pytest.raises(itofin.ItofinError):
        statistics.sortino_ratio([-1.0, 2.0], target, periods_per_year=12.0)


def test_performance_ratios_denominators_recovery_and_required_arguments():
    """No downside differs from zero dispersion; invalid calls leave no retained state."""
    assert statistics.target_downside_deviation([0.01, 0.02], 0.0) == 0.0
    assert statistics.sortino_ratio([-0.01, -0.01], 0.0, periods_per_year=1.0) == -1.0
    with pytest.raises(itofin.ItofinError):
        statistics.sortino_ratio([0.01, 0.02], 0.0, periods_per_year=12.0)
    with pytest.raises(itofin.ItofinError):
        statistics.sharpe_ratio([0.01, 0.01], 0.0, periods_per_year=12.0)
    with pytest.raises(itofin.ItofinError):
        statistics.target_downside_deviation([], 0.0)
    with pytest.raises(TypeError):
        statistics.sharpe_ratio([-1.0, 2.0], 0.0)  # type: ignore[call-arg]
    with pytest.raises(TypeError):
        statistics.sortino_ratio([-1.0, 2.0], 0.0)  # type: ignore[call-arg]
    with pytest.raises(TypeError):
        statistics.sortino_ratio([-1.0, 2.0], [0.0], periods_per_year=12.0)  # type: ignore[arg-type]
    assert math.isfinite(statistics.sortino_ratio([-1.0, 2.0], 0.0, periods_per_year=12.0))


def test_performance_ratios_extreme_scales():
    """Scaled squares avoid losing representable downside among extreme gains."""
    for scale in [1e-300, 1e300]:
        returns = [-scale, scale, 2 * scale]
        assert statistics.sortino_ratio(returns, 0.0, periods_per_year=1.0) == pytest.approx(
            2 / math.sqrt(3), rel=1e-13
        )
    assert statistics.target_downside_deviation([1e200, -1.0], 0.0) == pytest.approx(math.sqrt(0.5), rel=1e-13)
    assert statistics.sharpe_ratio([1.0, 2.0, 3.0], 1e16, periods_per_year=1.0) == pytest.approx(2.0 - 1e16, rel=1e-13)
