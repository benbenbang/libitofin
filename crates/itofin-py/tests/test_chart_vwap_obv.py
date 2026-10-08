"""Cumulative chart fixtures independently calculated from exact rational sums."""

# standard library
import json
import sys
from fractions import Fraction

# pypi/conda library
import numpy as np
import pytest

# itofin library
import itofin
from itofin import chart
from itofin.chart import obv, vwap


def test_vwap_obv_exact_rational_fixture_and_alignment():
    """Both indicators preserve bar alignment and their distinct first-valid rules."""
    price = [10.0, 12.0, 11.0, 11.0, 9.0]
    volume = [0.0, 2.0, 1.0, 0.0, 3.0]
    assert vwap is chart.vwap
    assert obv is chart.obv
    result = chart.vwap(price, volume)
    expected = [0.0, 12.0, float(Fraction(35, 3)), float(Fraction(35, 3)), float(Fraction(31, 3))]
    np.testing.assert_allclose(result.values, expected, rtol=0, atol=1e-12)
    assert result.values.dtype == np.float64
    assert result.first_valid == 1
    assert result.to_list()[0] is None
    assert result.to_list()[1] == 12.0
    signed = chart.obv(price, volume)
    assert signed.first_valid == 0
    assert signed.to_list() == [0.0, 2.0, 1.0, 1.0, -2.0]
    assert price == [10.0, 12.0, 11.0, 11.0, 9.0]
    assert volume == [0.0, 2.0, 1.0, 0.0, 3.0]


def test_vwap_obv_copies_and_caller_controlled_session_reset():
    """Input/result mutations do not leak, and each call starts fresh."""
    price = [2.0, 4.0, 8.0]
    volume = [1.0, 1.0, 2.0]
    result = chart.vwap(price, volume)
    price[0] = -999.0
    volume[0] = 999.0
    assert result.to_list() == [2.0, 3.0, 5.5]
    values = result.values
    values[:] = -1.0
    assert result.to_list() == [2.0, 3.0, 5.5]
    assert chart.vwap([8.0], [2.0]).to_list() == [8.0]
    assert chart.obv([8.0], [999.0]).to_list() == [0.0]


def test_empty_zero_volume_and_obv_equal_close_contracts():
    """Warmup is missing for VWAP but a zero seed is valid for OBV."""
    for function in (chart.vwap, chart.obv):
        empty = function([], [])
        assert empty.first_valid == 0
        assert empty.to_list() == []
    missing = chart.vwap([1.0, 2.0], [0.0, -0.0])
    assert missing.first_valid == 2
    assert missing.to_list() == [None, None]
    assert json.dumps(missing.to_list()) == "[null, null]"
    assert chart.vwap([100.0, 3.0, 999.0], [0.0, 2.0, 0.0]).to_list() == [None, 3.0, 3.0]
    assert chart.obv([1.0, 1.0, 2.0, 0.0], [999.0, 99.0, 0.0, 3.0]).to_list() == [0.0, 0.0, 0.0, -3.0]


@pytest.mark.parametrize("function", [chart.vwap, chart.obv])
@pytest.mark.parametrize(
    "price,volume",
    [
        ([], [1.0]),
        ([1.0], []),
        ([float("nan")], [0.0]),
        ([float("inf")], [0.0]),
        ([1.0], [float("nan")]),
        ([1.0], [float("-inf")]),
        ([1.0], [-1.0]),
    ],
)
def test_volume_lines_reject_invalid_inputs(function, price, volume):
    """Domain validation remains shared with Rust, C, and Go."""
    with pytest.raises(itofin.ItofinError):
        function(price, volume)


@pytest.mark.parametrize("function", [chart.vwap, chart.obv])
def test_volume_lines_reject_non_numeric_elements(function):
    """PyO3 extraction reports type errors before invoking the chart calculation."""
    with pytest.raises(TypeError):
        function(["not a price"], [1.0])
    with pytest.raises(TypeError):
        function([1.0], [None])


def test_extreme_prices_and_subnormal_weights_stay_finite():
    """Stable means avoid artificial product/difference overflow and underflow."""
    maximum = sys.float_info.max
    tiny = float.fromhex("0x0.0000000000001p-1022")
    assert chart.vwap([maximum, maximum], [2.0, 2.0]).to_list() == [maximum, maximum]
    assert chart.vwap([-maximum, maximum], [1.0, 1.0]).to_list() == [-maximum, 0.0]
    assert chart.vwap([3.0, 5.0], [tiny, tiny]).to_list() == [3.0, 4.0]
    assert chart.vwap([tiny], [tiny]).to_list() == [tiny]
    assert abs(chart.vwap([maximum, 0.0], [1.0, maximum]).values[1] - 1.0) < 1e-14
    assert chart.obv([-maximum, maximum, -maximum], [1.0, 2.0, 3.0]).to_list() == [0.0, 2.0, -1.0]


def test_volume_accumulation_overflow_is_rejected():
    """Finite prices/volumes do not permit nonfinite cumulative state."""
    maximum = sys.float_info.max
    with pytest.raises(itofin.ItofinError):
        chart.vwap([0.0, 0.0], [maximum, maximum])
    for prices in ([1.0, 2.0, 3.0], [3.0, 2.0, 1.0]):
        with pytest.raises(itofin.ItofinError):
            chart.obv(prices, [0.0, maximum, maximum])
