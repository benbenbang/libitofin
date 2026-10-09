"""Wilder directional indicators checked against exact Fraction step fixtures."""

# standard library
import csv
import sys
from fractions import Fraction
from pathlib import Path
from typing import Any, cast

# pypi/conda library
import numpy as np
import pytest

# itofin library
import itofin
from itofin import chart
from itofin.chart import Adx, adx


def test_adx_rational_fixture_alignment_and_readonly_ownership():
    """All four channels match a production-independent rational recurrence."""
    path = Path(__file__).parents[2] / "libitofin/tests/fixtures/chart_adx.csv"
    with path.open() as stream:
        rows = [[float(Fraction(value)) for value in row] for row in list(csv.reader(stream))[1:]]
    high, low, close = [[row[i] for row in rows] for i in range(3)]
    assert adx is chart.adx
    result = adx(high, low, close, period=2)
    assert isinstance(result, Adx)
    for k, series in enumerate((result.plus_di, result.minus_di, result.dx, result.adx)):
        valid = 3 if k == 3 else 2
        assert series.first_valid == valid
        assert series.to_list()[:valid] == [None] * valid
        assert series.values.dtype == np.float64
        np.testing.assert_allclose(series.values, [row[9 + k] for row in rows], rtol=0, atol=1e-12)
    high[0], low[0], close[0] = -99, -99, -99
    copied = result.plus_di.values
    copied[:] = -99
    assert abs(result.plus_di.values[2] - 200 / 7) < 1e-12
    assert abs(result.minus_di.values[2] - 200 / 7) < 1e-12
    with pytest.raises(AttributeError):
        cast(Any, result).adx = result.dx
    with pytest.raises(TypeError):
        cast(Any, Adx)()


def test_adx_default_empty_short_one_flat_and_directions():
    """Transition seed differs from ATR; first-valid indices mask warmup."""
    close = list(range(30))
    default = adx(close, close, close)
    explicit = adx(close, close, close, period=14)
    assert default.dx.first_valid == 14
    assert default.adx.first_valid == 27
    for a, b in zip(
        (default.plus_di, default.minus_di, default.dx, default.adx),
        (explicit.plus_di, explicit.minus_di, explicit.dx, explicit.adx),
    ):
        np.testing.assert_array_equal(a.values, b.values)
    for n in range(5):
        result = adx(close[:n], close[:n], close[:n], period=2)
        assert result.dx.first_valid == min(2, n)
        assert result.adx.first_valid == min(3, n)
    one = adx([1, 2], [1, 2], [1, 2], period=1)
    assert one.plus_di.to_list() == one.dx.to_list() == one.adx.to_list() == [None, 100.0]
    assert one.minus_di.to_list() == [None, 0.0]
    flat = adx([-3] * 4, [-3] * 4, [-3] * 4, period=2)
    assert flat.dx.to_list() == [None, None, 0.0, 0.0]
    assert flat.adx.to_list() == [None, None, None, 0.0]
    zero_sum = adx([2] * 4, [-4] * 4, [-3] * 4, period=2)
    assert zero_sum.dx.to_list() == flat.dx.to_list()
    for sign in (1, -1):
        prices = [sign * i for i in range(8)]
        result = adx(prices, prices, prices, period=2)
        assert result.dx.to_list()[2:] == [100.0] * 6
        assert result.adx.to_list()[3:] == [100.0] * 5
    maximum = adx([0, sys.float_info.max, sys.float_info.max], [0] * 3, [0] * 3, period=2)
    assert np.isfinite(maximum.plus_di.values[2])


@pytest.mark.parametrize(
    "high,low,close",
    [
        ([], [0], [1]),
        ([2], [], [1]),
        ([2], [0], []),
        ([float("nan")], [0], [1]),
        ([2], [float("-inf")], [1]),
        ([2], [0], [float("inf")]),
        ([2], [3], [1]),
        ([sys.float_info.max], [-sys.float_info.max], [0]),
        ([sys.float_info.max, -sys.float_info.max], [0, -sys.float_info.max], [0, -sys.float_info.max]),
    ],
)
def test_adx_invalid_inputs_and_directional_overflow_during_warmup(high, low, close):
    """Finite raw operands are insufficient if a discarded movement overflows."""
    with pytest.raises(itofin.ItofinError):
        adx(high, low, close)


def test_adx_parameter_and_conversion_errors():
    """Period zero is a shared numerical error, negative periods fail extraction."""
    with pytest.raises(itofin.ItofinError):
        adx([], [], [], period=0)
    with pytest.raises(OverflowError):
        adx([], [], [], period=-1)
    with pytest.raises(TypeError):
        adx(cast(Any, ["not a price"]), [0], [1])
