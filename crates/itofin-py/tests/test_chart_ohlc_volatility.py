"""Pointwise OHLC estimators expose aligned float64 chart series."""

import numpy as np
import pytest

import itofin
from itofin import chart


FIELDS = (
    "simple_sigma",
    "parkinson_sigma",
    "garman_klass_sigma4",
    "garman_klass_sigma5",
)


def test_ohlc_point_fixture_and_fraction_forms():
    """The four estimators match the core fixture in both fraction forms."""
    fraction = 1.0 / 252.0
    indexed = chart.ohlc_point_volatility(
        [100.0], [110.0], [90.0], [105.0], [fraction]
    )
    scalar = chart.ohlc_point_volatility_constant_fraction(
        [100.0], [110.0], [90.0], [105.0], fraction
    )
    expected = (
        0.7745198449099887,
        1.9131168640323526,
        2.204975405342331,
        2.2004838300550182,
    )

    for result in (indexed, scalar):
        assert isinstance(result, chart.OhlcPointEstimates)
        for field, value in zip(FIELDS, expected, strict=True):
            series = getattr(result, field)
            assert isinstance(series, chart.ChartSeries)
            assert series.first_valid == 0
            assert series.values.dtype == np.float64
            assert series.to_list() == pytest.approx([value], abs=1e-12)
            np.testing.assert_allclose(series.values, [value], rtol=0.0, atol=1e-12)

    with pytest.raises(AttributeError):
        setattr(indexed, "simple_sigma", scalar.simple_sigma)


def test_ohlc_point_empty_short_and_copies():
    """Empty and single-bar inputs are usable without warming up or mutation."""
    empty = chart.ohlc_point_volatility([], [], [], [], [])
    for field in FIELDS:
        series = getattr(empty, field)
        assert series.first_valid == 0
        assert series.values.dtype == np.float64
        assert series.to_list() == []

    open_ = [100.0, 101.0]
    high = [110.0, 111.0]
    low = [90.0, 91.0]
    close = [105.0, 106.0]
    fractions = [1.0 / 252.0] * 2
    originals = [values.copy() for values in (open_, high, low, close, fractions)]
    indexed = chart.ohlc_point_volatility(open_, high, low, close, fractions)
    scalar = chart.ohlc_point_volatility_constant_fraction(
        open_, high, low, close, fractions[0]
    )
    assert [open_, high, low, close, fractions] == originals

    for field in FIELDS:
        a = getattr(indexed, field)
        b = getattr(scalar, field)
        assert a.first_valid == b.first_valid == 0
        assert len(a.values) == len(b.values) == 2
        np.testing.assert_allclose(a.values, b.values, rtol=0.0, atol=1e-12)
        copied_values = a.values
        copied_values[0] = -1.0
        assert a.values[0] >= 0.0


@pytest.mark.parametrize(
    "open_, high, low, close, fractions",
    [
        ([100.0], [], [90.0], [105.0], [1.0]),
        ([100.0], [110.0], [], [105.0], [1.0]),
        ([100.0], [110.0], [90.0], [], [1.0]),
        ([100.0], [110.0], [90.0], [105.0], []),
        ([100.0], [110.0], [90.0], [105.0], [0.0]),
        ([100.0], [110.0], [90.0], [105.0], [float("nan")]),
        ([100.0], [110.0], [90.0], [105.0], [float("inf")]),
        ([100.0], [99.0], [90.0], [105.0], [1.0]),
        ([100.0], [110.0], [101.0], [105.0], [1.0]),
        ([100.0], [110.0], [90.0], [111.0], [1.0]),
        ([100.0], [float("nan")], [90.0], [105.0], [1.0]),
        ([-100.0], [-90.0], [-110.0], [-95.0], [1.0]),
    ],
)
def test_ohlc_point_invalid_indexed_inputs(open_, high, low, close, fractions):
    """Length, range, positivity, and fraction errors use ItofinError."""
    with pytest.raises(itofin.ItofinError):
        chart.ohlc_point_volatility(open_, high, low, close, fractions)


@pytest.mark.parametrize("fraction", [0.0, -1.0, float("nan"), float("inf")])
def test_ohlc_point_invalid_scalar_fraction(fraction):
    """The scalar form rejects nonpositive and nonfinite fractions."""
    with pytest.raises(itofin.ItofinError):
        chart.ohlc_point_volatility_constant_fraction(
            [100.0], [110.0], [90.0], [105.0], fraction
        )
