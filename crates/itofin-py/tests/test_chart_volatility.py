"""Close-only volatility estimators preserve chart alignment and core errors."""

import numpy as np
import pytest

import itofin
from itofin import chart


def test_simple_local_volatility_fixture_and_scalar_fraction():
    """Both input forms reproduce the close-only fixture and chart warmup."""
    close = [100.0, 110.0, 99.0]
    fraction = 1.0 / 252.0
    expected = [1.5130021990505673, 1.6725463346168112]

    indexed = chart.simple_local_volatility(close, [0.0, fraction, fraction])
    scalar = chart.simple_local_volatility_constant_fraction(close, fraction)
    for series in (indexed, scalar):
        assert isinstance(series, chart.ChartSeries)
        assert series.first_valid == 1
        assert series.values.dtype == np.float64
        assert series.to_list()[0] is None
        np.testing.assert_allclose(series.values[1:], expected, rtol=0.0, atol=1e-12)

    np.testing.assert_allclose(indexed.values, scalar.values, rtol=0.0, atol=1e-12)


def test_constant_volatility_uses_preceding_valid_values():
    """A window starts after the required number of earlier valid bars."""
    local = chart.simple_local_volatility_constant_fraction([100.0, 110.0, 99.0], 1.0 / 252.0)
    constant = chart.constant_volatility(local, 1)
    assert constant.first_valid == 2
    assert constant.to_list()[:2] == [None, None]
    np.testing.assert_allclose(constant.values[2], 1.0698541148988145, rtol=0.0, atol=1e-12)

    too_long = chart.constant_volatility(local, 2)
    assert too_long.first_valid == 3
    assert too_long.to_list() == [None, None, None]

    flat = chart.simple_local_volatility_constant_fraction([100.0] * 3, 1.0 / 252.0)
    assert chart.constant_volatility(flat, 1).to_list() == [None, None, 0.0]


def test_volatility_empty_short_and_invalid_inputs():
    """Empty and warmup results stay aligned, and bad data uses ItofinError."""
    empty = chart.simple_local_volatility([], [])
    assert empty.first_valid == 0
    assert empty.to_list() == []
    assert chart.simple_local_volatility_constant_fraction([], 1.0).to_list() == []
    assert chart.constant_volatility(empty, 1).to_list() == []

    one = chart.simple_local_volatility_constant_fraction([100.0], 1.0)
    assert one.first_valid == 1
    assert one.to_list() == [None]
    assert chart.constant_volatility(one, 1).to_list() == [None]

    with pytest.raises(itofin.ItofinError):
        chart.simple_local_volatility([100.0, 110.0], [1.0])
    with pytest.raises(itofin.ItofinError):
        chart.simple_local_volatility([100.0, 110.0], [0.0, 0.0])
    with pytest.raises(itofin.ItofinError):
        chart.simple_local_volatility([100.0, float("nan")], [0.0, 1.0])
    with pytest.raises(itofin.ItofinError):
        chart.simple_local_volatility_constant_fraction([100.0, 0.0], 1.0)
    with pytest.raises(itofin.ItofinError):
        chart.simple_local_volatility_constant_fraction([], 0.0)
    with pytest.raises(itofin.ItofinError):
        chart.simple_local_volatility_constant_fraction([100.0, 110.0], float("inf"))
    with pytest.raises(itofin.ItofinError):
        chart.constant_volatility(one, 0)
    with pytest.raises(itofin.ItofinError):
        chart.constant_volatility(one, -1)
