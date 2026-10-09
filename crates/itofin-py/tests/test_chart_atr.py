"""Gap-aware ATR fixtures independently calculated with exact rational arithmetic."""

# standard library
import json
import sys
from fractions import Fraction
from typing import Any, cast

# pypi/conda library
import numpy as np
import pytest

# itofin library
import itofin
from itofin import chart
from itofin.chart import atr, true_range


def test_atr_true_range_independent_fixture_and_copies():
    """All bars align, with a separate missing prefix for Wilder ATR."""
    high = [12.0, 16.0, 11.0, 15.0, 14.0, 14.0]
    low = [10.0, 14.0, 9.0, 13.0, 12.0, 14.0]
    close = [11.0, 15.0, 10.0, 14.0, 13.0, 14.0]
    assert atr is chart.atr
    assert true_range is chart.true_range
    ranges = true_range(high, low, close)
    assert ranges.first_valid == 0
    assert ranges.to_list() == [2.0, 5.0, 6.0, 5.0, 2.0, 1.0]
    result = atr(high, low, close, period=3)
    expected = [0.0, 0.0, float(Fraction(13, 3)), float(Fraction(41, 9)), float(Fraction(100, 27)), float(Fraction(227, 81))]
    np.testing.assert_allclose(result.values, expected, rtol=0, atol=1e-12)
    assert result.first_valid == 2
    assert result.values.dtype == np.float64
    assert result.to_list()[:2] == [None, None]
    high[0], low[0], close[0] = -99.0, -99.0, -99.0
    values = result.values
    values[:] = -99.0
    np.testing.assert_allclose(result.values, expected, rtol=0, atol=1e-12)
    assert ranges.to_list() == [2.0, 5.0, 6.0, 5.0, 2.0, 1.0]


def test_atr_empty_short_and_fourteen_bar_default():
    """Default arithmetic seeding includes bar zero; short data is all missing."""
    for function in (atr, true_range):
        empty = function([], [], [])
        assert empty.first_valid == 0
        assert empty.to_list() == []
    short = atr([2.0, 3.0], [0.0, 1.0], [1.0, 2.0], period=3)
    assert short.first_valid == 2
    assert short.to_list() == [None, None]
    assert json.dumps(short.to_list()) == "[null, null]"
    high = list(range(1, 16))
    result = atr(high, [0.0] * 15, [0.0] * 15)
    explicit = atr(high, [0.0] * 15, [0.0] * 15, period=14)
    assert result.first_valid == 13
    np.testing.assert_array_equal(result.values, explicit.values)
    assert result.values[13] == 7.5
    assert abs(result.values[14] - float(Fraction(225, 28))) < 1e-12


def test_atr_flat_negative_prices_and_finite_extremes():
    """Period one avoids cancellation and representable means avoid seed overflow."""
    flat = atr([-3.0] * 4, [-3.0] * 4, [-3.0] * 4, period=2)
    assert flat.to_list() == [None, 0.0, 0.0, 0.0]
    maximum = sys.float_info.max
    tiny = float.fromhex("0x0.0000000000001p-1022")
    one = atr([maximum, tiny], [0.0, 0.0], [0.0, 0.0], period=1)
    assert one.to_list() == [maximum, tiny]
    seed = atr([maximum] * 3, [0.0] * 3, [0.0] * 3, period=3)
    assert seed.to_list() == [None, None, maximum]


@pytest.mark.parametrize("function", [atr, true_range])
@pytest.mark.parametrize(
    "high,low,close",
    [
        ([], [0.0], [1.0]),
        ([2.0], [], [1.0]),
        ([2.0], [0.0], []),
        ([float("nan")], [0.0], [1.0]),
        ([2.0], [float("-inf")], [1.0]),
        ([2.0], [0.0], [float("inf")]),
        ([2.0], [3.0], [1.0]),
        ([2.0], [0.0], [3.0]),
        ([sys.float_info.max], [-sys.float_info.max], [0.0]),
        ([-sys.float_info.max, sys.float_info.max], [-sys.float_info.max, 0.0], [-sys.float_info.max, 0.0]),
        ([sys.float_info.max, 0.0], [sys.float_info.max, -sys.float_info.max], [sys.float_info.max, 0.0]),
    ],
)
def test_atr_true_range_reject_invalid_hlc_and_each_overflow_candidate(function, high, low, close):
    """ATR rejects invalid ranges during warmup rather than suppressing them."""
    with pytest.raises(itofin.ItofinError):
        function(high, low, close)


def test_atr_invalid_period_and_nonnumeric_input():
    """Scalar period and PyO3 conversion errors remain explicit."""
    with pytest.raises(itofin.ItofinError):
        atr([], [], [], period=0)
    with pytest.raises(OverflowError):
        atr([], [], [], period=-1)
    for function in (atr, true_range):
        with pytest.raises(TypeError):
            function(cast(Any, ["not a price"]), [0.0], [1.0])
