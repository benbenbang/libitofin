"""Overnight OHLC estimators expose aligned float64 chart series."""

import numpy as np
import pytest

import itofin
from itofin import chart


FIELDS = (
    "garman_klass_sigma1",
    "garman_klass_sigma3",
    "garman_klass_sigma6",
)
OPEN = [100.0, 110.0]
HIGH = [100.0, 120.0]
LOW = [100.0, 105.0]
CLOSE = [100.0, 115.0]
YEAR_FRACTION = 1.0 / 252.0
OVERNIGHT_FRACTION = 0.25


def test_ohlc_overnight_fixture_and_fraction_forms():
    """All three estimators match the independent overnight fixture."""
    indexed = chart.ohlc_overnight_volatility(OPEN, HIGH, LOW, CLOSE, [float("nan"), YEAR_FRACTION], OVERNIGHT_FRACTION)
    scalar = chart.ohlc_overnight_volatility_constant_fraction(
        OPEN, HIGH, LOW, CLOSE, YEAR_FRACTION, OVERNIGHT_FRACTION
    )
    expected = (
        2.2159224836472786,
        1.8303355611439194,
        1.6795672145926133,
    )
    for result in (indexed, scalar):
        assert isinstance(result, chart.OhlcOvernightEstimates)
        for field, value in zip(FIELDS, expected, strict=True):
            series = getattr(result, field)
            assert isinstance(series, chart.ChartSeries)
            assert series.first_valid == 1
            assert series.values.dtype == np.float64
            assert series.to_list()[0] is None
            assert series.to_list()[1] == pytest.approx(value, abs=1e-12)
            np.testing.assert_allclose(series.values, [0.0, value], rtol=0.0, atol=1e-12)

    with pytest.raises(AttributeError):
        setattr(indexed, "garman_klass_sigma1", scalar.garman_klass_sigma1)


def test_ohlc_overnight_empty_short_and_copies():
    """A preceding close is required and caller arrays are never mutated."""
    empty = chart.ohlc_overnight_volatility([], [], [], [], [], OVERNIGHT_FRACTION)
    one = chart.ohlc_overnight_volatility_constant_fraction(
        [100.0], [110.0], [90.0], [105.0], YEAR_FRACTION, OVERNIGHT_FRACTION
    )
    for field in FIELDS:
        empty_series = getattr(empty, field)
        one_series = getattr(one, field)
        assert empty_series.first_valid == 0
        assert empty_series.values.dtype == np.float64
        assert empty_series.to_list() == []
        assert one_series.first_valid == 1
        assert one_series.values.dtype == np.float64
        assert one_series.to_list() == [None]
        np.testing.assert_allclose(one_series.values, [0.0])

    open_ = OPEN.copy()
    high = HIGH.copy()
    low = LOW.copy()
    close = CLOSE.copy()
    fractions = [0.0, YEAR_FRACTION]
    originals = [values.copy() for values in (open_, high, low, close, fractions)]
    indexed = chart.ohlc_overnight_volatility(open_, high, low, close, fractions, OVERNIGHT_FRACTION)
    scalar = chart.ohlc_overnight_volatility_constant_fraction(
        open_, high, low, close, YEAR_FRACTION, OVERNIGHT_FRACTION
    )
    assert [open_, high, low, close, fractions] == originals
    for field in FIELDS:
        a = getattr(indexed, field)
        b = getattr(scalar, field)
        assert a.first_valid == b.first_valid == 1
        np.testing.assert_allclose(a.values, b.values, rtol=0.0, atol=1e-12)
        copied_values = a.values
        copied_values[1] = -1.0
        assert a.values[1] >= 0.0


@pytest.mark.parametrize(
    "open_, high, low, close, fractions, overnight_fraction",
    [
        (OPEN, HIGH[:1], LOW, CLOSE, [0.0, YEAR_FRACTION], OVERNIGHT_FRACTION),
        (OPEN, HIGH, LOW[:1], CLOSE, [0.0, YEAR_FRACTION], OVERNIGHT_FRACTION),
        (OPEN, HIGH, LOW, CLOSE[:1], [0.0, YEAR_FRACTION], OVERNIGHT_FRACTION),
        (OPEN, HIGH, LOW, CLOSE, [YEAR_FRACTION], OVERNIGHT_FRACTION),
        (OPEN, HIGH, LOW, CLOSE, [0.0, 0.0], OVERNIGHT_FRACTION),
        (OPEN, HIGH, LOW, CLOSE, [0.0, float("nan")], OVERNIGHT_FRACTION),
        (OPEN, HIGH, LOW, CLOSE, [0.0, float("inf")], OVERNIGHT_FRACTION),
        (OPEN, [100.0, 109.0], LOW, CLOSE, [0.0, YEAR_FRACTION], OVERNIGHT_FRACTION),
        (OPEN, HIGH, [100.0, 111.0], CLOSE, [0.0, YEAR_FRACTION], OVERNIGHT_FRACTION),
        (OPEN, HIGH, LOW, [100.0, 121.0], [0.0, YEAR_FRACTION], OVERNIGHT_FRACTION),
        ([0.0, 110.0], HIGH, LOW, CLOSE, [0.0, YEAR_FRACTION], OVERNIGHT_FRACTION),
        (OPEN, HIGH, LOW, CLOSE, [0.0, YEAR_FRACTION], 0.0),
        (OPEN, HIGH, LOW, CLOSE, [0.0, YEAR_FRACTION], 1.0),
        (OPEN, HIGH, LOW, CLOSE, [0.0, YEAR_FRACTION], -0.25),
        (OPEN, HIGH, LOW, CLOSE, [0.0, YEAR_FRACTION], float("nan")),
        (OPEN, HIGH, LOW, CLOSE, [0.0, YEAR_FRACTION], float("inf")),
    ],
)
def test_ohlc_overnight_invalid_indexed_inputs(open_, high, low, close, fractions, overnight_fraction):
    """Length, range, positivity, and fraction errors use ItofinError."""
    with pytest.raises(itofin.ItofinError):
        chart.ohlc_overnight_volatility(open_, high, low, close, fractions, overnight_fraction)


@pytest.mark.parametrize("fraction", [0.0, -1.0, float("nan"), float("inf")])
def test_ohlc_overnight_invalid_scalar_fraction(fraction):
    """The scalar form rejects nonpositive and nonfinite year fractions."""
    with pytest.raises(itofin.ItofinError):
        chart.ohlc_overnight_volatility_constant_fraction(OPEN, HIGH, LOW, CLOSE, fraction, OVERNIGHT_FRACTION)
