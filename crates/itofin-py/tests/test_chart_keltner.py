"""Modern Keltner channel fixtures from exact rational EMA and Wilder recurrences."""

# standard library
import sys
from fractions import Fraction
from typing import Any, cast

# pypi/conda library
import numpy as np
import pytest

# itofin library
import itofin
from itofin import chart
from itofin.chart import KeltnerChannels, keltner_channels


def test_keltner_rational_fixture_and_readonly_ownership():
    """Close-price EMA and separate ATR periods preserve the hand fixture."""
    high = [12.0, 16.0, 11.0, 15.0, 14.0, 14.0]
    low = [10.0, 14.0, 9.0, 13.0, 12.0, 14.0]
    close = [11.0, 15.0, 10.0, 14.0, 13.0, 14.0]
    assert keltner_channels is chart.keltner_channels
    result = keltner_channels(high, low, close, center_period=3, atr_period=2, multiplier=1.5)
    assert isinstance(result, KeltnerChannels)
    expected = [
        [0.0, 0.0, 12.0, 13.0, 13.0, 13.5],
        [0.0, 0.0, float(Fraction(153, 8)), float(Fraction(325, 16)), float(Fraction(581, 32)), float(Fraction(1077, 64))],
        [0.0, 0.0, float(Fraction(39, 8)), float(Fraction(91, 16)), float(Fraction(251, 32)), float(Fraction(651, 64))],
    ]
    for series, values in zip((result.center, result.upper, result.lower), expected):
        assert series.first_valid == 2
        assert series.to_list()[:2] == [None, None]
        assert series.values.dtype == np.float64
        np.testing.assert_allclose(series.values, values, rtol=0, atol=1e-12)
    high[0], low[0], close[0] = -99.0, -99.0, -99.0
    copy = result.upper.values
    copy[:] = -99.0
    np.testing.assert_allclose(result.upper.values, expected[1], rtol=0, atol=1e-12)
    with pytest.raises(AttributeError):
        cast(Any, result).center = result.lower
    with pytest.raises(TypeError):
        cast(Any, KeltnerChannels)()


def test_keltner_defaults_and_combined_warmup():
    """Defaults are EMA 20, ATR 10, multiplier 2, not standalone ATR 14."""
    close = list(range(1, 25))
    result = keltner_channels(close, close, close)
    explicit = keltner_channels(close, close, close, center_period=20, atr_period=10, multiplier=2)
    for series, other in zip((result.center, result.upper, result.lower), (explicit.center, explicit.upper, explicit.lower)):
        assert series.first_valid == 19
        np.testing.assert_array_equal(series.values, other.values)
        assert series.to_list()[:19] == [None] * 19
    fourteen = keltner_channels(close, close, close, atr_period=14)
    assert result.upper.values[-1] != fourteen.upper.values[-1]
    for center_period, atr_period in [(2, 4), (4, 2)]:
        zero = keltner_channels(close, close, close, center_period, atr_period, multiplier=0)
        assert zero.center.first_valid == 3
        np.testing.assert_array_equal(zero.center.values, zero.upper.values)
        np.testing.assert_array_equal(zero.center.values, zero.lower.values)


def test_keltner_empty_short_flat_period_one_and_zero_multiplier():
    """Missing warmup, negative flat prices and gap-aware period-one width align."""
    empty = keltner_channels([], [], [])
    for series in (empty.center, empty.upper, empty.lower):
        assert series.first_valid == 0
        assert series.to_list() == []
    short = keltner_channels([2], [0], [1], center_period=1, atr_period=3)
    for series in (short.center, short.upper, short.lower):
        assert series.first_valid == 1
        assert series.to_list() == [None]
    flat = keltner_channels([-3] * 3, [-3] * 3, [-3] * 3, center_period=2, atr_period=1)
    assert flat.center.to_list() == flat.upper.to_list() == flat.lower.to_list() == [None, -3.0, -3.0]
    one = keltner_channels([2, 6], [0, 4], [1, 5], center_period=1, atr_period=1)
    assert one.center.to_list() == [1.0, 5.0]
    assert one.upper.to_list() == [5.0, 15.0]
    assert one.lower.to_list() == [-3.0, -5.0]
    maximum = sys.float_info.max
    zero = keltner_channels([maximum], [0], [maximum], center_period=1, atr_period=1, multiplier=0)
    assert zero.center.to_list() == zero.upper.to_list() == zero.lower.to_list() == [maximum]


@pytest.mark.parametrize("parameters", [{"center_period": 0}, {"atr_period": 0}, {"multiplier": -1}, {"multiplier": float("nan")}, {"multiplier": float("inf")}, {"multiplier": float("-inf")}])
def test_keltner_reject_invalid_parameters_even_empty(parameters):
    """Explicit invalid options fail without depending on data length."""
    with pytest.raises(itofin.ItofinError):
        keltner_channels([], [], [], **parameters)


@pytest.mark.parametrize("high,low,close", [([], [0], [1]), ([2], [], [1]), ([2], [0], []), ([float("nan")], [0], [1]), ([2], [float("-inf")], [1]), ([2], [0], [float("inf")]), ([2], [3], [1]), ([sys.float_info.max], [-sys.float_info.max], [0])])
def test_keltner_reject_invalid_hlc_and_range_overflow_during_warmup(high, low, close):
    """A zero multiplier does not suppress input or raw-range validation."""
    with pytest.raises(itofin.ItofinError):
        keltner_channels(high, low, close, multiplier=0)


@pytest.mark.parametrize("high,low,close,multiplier", [(sys.float_info.max, 0, 0, 2), (sys.float_info.max, 0, sys.float_info.max, 1), (0, -sys.float_info.max, -sys.float_info.max, 1)])
def test_keltner_reject_offset_and_both_band_overflows(high, low, close, multiplier):
    """Finite operands cannot return an infinite envelope."""
    with pytest.raises(itofin.ItofinError):
        keltner_channels([high], [low], [close], center_period=1, atr_period=1, multiplier=multiplier)


def test_keltner_nonnumeric_inputs_and_negative_period_extraction():
    """PyO3 conversion errors remain distinct from shared numerical errors."""
    with pytest.raises(TypeError):
        keltner_channels(cast(Any, ["not a price"]), [0], [1])
    with pytest.raises(OverflowError):
        keltner_channels([], [], [], center_period=-1)
    with pytest.raises(OverflowError):
        keltner_channels([], [], [], atr_period=-1)


def test_keltner_center_uses_close_not_typical_price():
    """Asymmetric bars distinguish the close EMA seed 3 from typical-price 6."""
    result = keltner_channels([9, 12, 15], [0, 3, 6], [0, 3, 6], center_period=3, atr_period=2, multiplier=0)
    assert result.center.to_list() == [None, None, 3.0]
