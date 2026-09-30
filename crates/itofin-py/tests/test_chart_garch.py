"""GARCH(1,1) fitting, filtering, and forecasting via Python."""

import json
import math
from pathlib import Path

import numpy as np
import pytest

import itofin
from itofin import chart


ALPHA = 0.2
BETA = 0.3
LONG_RUN_VARIANCE = 0.4


def test_garch11_filter_repeated_returns_fixture():
    """Conditional volatilities follow the documented GARCH recurrence."""
    returns = [0.1, 0.1, 0.1]
    result = chart.garch11_filter(returns, ALPHA, BETA, LONG_RUN_VARIANCE)

    assert isinstance(result, chart.Garch11Result)
    assert isinstance(result.conditional_volatility, chart.ChartSeries)
    assert result.conditional_volatility.first_valid == 1
    assert result.conditional_volatility.values.dtype == np.float64
    assert result.conditional_volatility.to_list()[0] is None
    np.testing.assert_allclose(
        result.conditional_volatility.values,
        [0.0, math.sqrt(0.205), math.sqrt(0.2635)],
        rtol=0.0,
        atol=1e-14,
    )
    assert result.next_variance == pytest.approx(0.28105, abs=1e-14)
    assert returns == [0.1, 0.1, 0.1]

    copied_values = result.conditional_volatility.values
    copied_values[1] = -1.0
    assert result.conditional_volatility.values[1] > 0.0
    with pytest.raises(AttributeError):
        setattr(result, "next_variance", -1.0)


def test_garch11_filter_uses_preceding_return():
    result = chart.garch11_filter([0.1, 0.2, 0.3], ALPHA, BETA, LONG_RUN_VARIANCE)
    np.testing.assert_allclose(
        result.conditional_volatility.values,
        [0.0, math.sqrt(0.205), math.sqrt(0.2695)],
        rtol=0.0,
        atol=1e-14,
    )
    assert result.next_variance == pytest.approx(0.29885, abs=1e-14)


def test_garch11_one_return_and_forecast_agree():
    """A single return has only a warmup slot and produces its next variance."""
    result = chart.garch11_filter([0.1], ALPHA, BETA, LONG_RUN_VARIANCE)
    assert result.conditional_volatility.first_valid == 1
    assert result.conditional_volatility.to_list() == [None]
    np.testing.assert_array_equal(result.conditional_volatility.values, [0.0])
    assert result.next_variance == pytest.approx(0.205, abs=1e-14)
    assert chart.garch11_forecast(0.1, 0.01, ALPHA, BETA, LONG_RUN_VARIANCE) == pytest.approx(
        result.next_variance, abs=1e-14
    )


@pytest.mark.parametrize(
    "alpha, beta, long_run_variance",
    [
        (-0.1, BETA, LONG_RUN_VARIANCE),
        (ALPHA, -0.1, LONG_RUN_VARIANCE),
        (0.7, BETA, LONG_RUN_VARIANCE),
        (ALPHA, BETA, -0.01),
        (float("nan"), BETA, LONG_RUN_VARIANCE),
        (ALPHA, float("inf"), LONG_RUN_VARIANCE),
        (ALPHA, BETA, float("nan")),
    ],
)
def test_garch11_rejects_invalid_parameters(alpha, beta, long_run_variance):
    """Both Python entry points report model validation as ItofinError."""
    with pytest.raises(itofin.ItofinError):
        chart.garch11_filter([0.1, 0.1], alpha, beta, long_run_variance)
    with pytest.raises(itofin.ItofinError):
        chart.garch11_forecast(0.1, 0.01, alpha, beta, long_run_variance)


@pytest.mark.parametrize("returns", [[0.1, float("nan")], [0.1, float("inf")], [1e308, 0.1]])
def test_garch11_filter_rejects_nonfinite_returns_and_overflow(returns):
    """Nonfinite inputs and nonfinite recurrence outcomes are errors."""
    with pytest.raises(itofin.ItofinError):
        chart.garch11_filter(returns, ALPHA, BETA, LONG_RUN_VARIANCE)


def test_garch11_filter_rejects_empty_returns():
    """The filter requires an observed return to seed its variance."""
    with pytest.raises(itofin.ItofinError):
        chart.garch11_filter([], ALPHA, BETA, LONG_RUN_VARIANCE)


@pytest.mark.parametrize(
    "last_return, current_variance",
    [
        (float("nan"), 0.01),
        (float("inf"), 0.01),
        (0.1, -0.04),
        (0.1, float("nan")),
        (1e308, 0.01),
    ],
)
def test_garch11_forecast_rejects_invalid_state(last_return, current_variance):
    """Forecast inputs cannot produce invalid conditional variance."""
    with pytest.raises(itofin.ItofinError):
        chart.garch11_forecast(last_return, current_variance, ALPHA, BETA, LONG_RUN_VARIANCE)


def test_garch11_fit_deterministic_returns():
    """Fitting is reproducible and returns a frozen stationary model snapshot."""
    returns = [0.01 * math.sin(index * 1.7) + 0.007 * math.cos(index * 0.31) for index in range(160)]
    original_returns = returns.copy()

    result = chart.garch11_fit(returns)
    repeated = chart.garch11_fit(returns)

    assert isinstance(result, chart.Garch11FitResult)
    assert 0.0 <= result.alpha < 1.0
    assert 0.0 <= result.beta < 1.0
    assert result.alpha + result.beta < 1.0
    assert result.omega > 0.0
    assert math.isfinite(result.log_likelihood)
    assert math.isfinite(result.next_variance)
    assert result.next_variance > 0.0
    filtered = chart.garch11_filter(
        returns,
        result.alpha,
        result.beta,
        result.omega / (1.0 - result.alpha - result.beta),
    )
    assert result.next_variance == pytest.approx(filtered.next_variance, rel=1e-12)
    assert (
        result.alpha,
        result.beta,
        result.omega,
        result.log_likelihood,
        result.next_variance,
    ) == (
        repeated.alpha,
        repeated.beta,
        repeated.omega,
        repeated.log_likelihood,
        repeated.next_variance,
    )
    assert returns == original_returns
    with pytest.raises(AttributeError):
        setattr(result, "alpha", 0.0)


def test_garch11_fit_quantlib_oracle():
    fixture = Path(__file__).resolve().parents[3] / "crates/libitofin/tests/fixtures/garch_fit"
    oracle = json.loads((fixture / "oracle.json").read_text())
    returns = np.fromfile(fixture / "returns.bin", dtype="<f8")
    assert len(returns) == oracle["input"]["count"]

    result = chart.garch11_fit(returns.tolist())
    for field, expected in oracle["expected"].items():
        assert getattr(result, field) == pytest.approx(expected, abs=1e-6)


@pytest.mark.parametrize(
    "returns",
    [
        [],
        [0.01, -0.02, 0.015],
        [0.0] * 6,
        [0.01, -0.02, float("nan"), -0.025, 0.005, -0.01],
        [0.01, -0.02, float("inf"), -0.025, 0.005, -0.01],
        [0.01, -0.02, 1e308, -0.025, 0.005, -0.01],
    ],
)
def test_garch11_fit_rejects_invalid_returns(returns):
    """Invalid input series raise the same error type as fixed-parameter filtering."""
    with pytest.raises(itofin.ItofinError):
        chart.garch11_fit(returns)


def test_garch11_fit_rejects_too_many_returns():
    with pytest.raises(itofin.ItofinError, match="at most 100000 returns"):
        chart.garch11_fit([0.0] * 100_001)
