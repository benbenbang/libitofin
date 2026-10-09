"""Williams percent R parity against an independent exact rational fixture."""

# standard library
import csv
import json
import sys
from pathlib import Path
from typing import Any, cast

# pypi/conda library
import numpy as np
import pytest

# itofin library
import itofin
from itofin import chart
from itofin.chart import williams_r


def test_williams_r_independent_fixture_and_owned_copies():
    """Inclusive trailing extrema expire; result and property arrays are owned."""
    path = Path(__file__).parents[2] / "libitofin/tests/data/chart/williams_r.csv"
    with path.open(newline="") as stream:
        rows = [[float(value) for value in row] for row in list(csv.reader(stream))[1:]]
    high, low, close, expected = map(list, zip(*rows))
    assert williams_r is chart.williams_r
    result = williams_r(high, low, close, period=3)
    np.testing.assert_allclose(result.values, expected, rtol=0, atol=1e-12)
    assert result.first_valid == 2
    assert result.values.dtype == np.float64
    assert result.to_list()[:2] == [None, None]
    assert result.to_list()[6] == 0.0
    high[0], low[0], close[0] = -99.0, -99.0, -99.0
    values = result.values
    values[:] = -99.0
    np.testing.assert_allclose(result.values, expected, rtol=0, atol=1e-12)


def test_williams_r_defaults_empty_short_and_extreme_finite():
    """Warmup remains missing, flat windows are neutral, and bounds are exact."""
    empty = williams_r([], [], [])
    assert empty.first_valid == 0 and empty.to_list() == []
    short = williams_r([2.0], [0.0], [1.0], period=3)
    assert short.first_valid == 1
    assert json.dumps(short.to_list()) == "[null]"
    result = williams_r([-3.0] * 15, [-3.0] * 15, [-3.0] * 15)
    assert result.first_valid == 13
    assert result.to_list()[13:] == [-50.0, -50.0]
    explicit = williams_r([-3.0] * 15, [-3.0] * 15, [-3.0] * 15, period=14)
    np.testing.assert_array_equal(result.values, explicit.values)
    assert williams_r([2.0, 2.0], [0.0, 0.0], [2.0, 0.0], period=1).to_list() == [0.0, -100.0]
    rising = [1.0, 2.0, 3.0]
    falling = rising[::-1]
    assert williams_r(rising, rising, rising, period=2).to_list() == [None, 0.0, 0.0]
    assert williams_r(falling, falling, falling, period=2).to_list() == [None, -100.0, -100.0]
    tiny = float.fromhex("0x0.0000000000001p-1022")
    assert williams_r([tiny, sys.float_info.max], [0.0, 0.0], [0.0, 0.0], period=1).to_list() == [-100.0, -100.0]


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
        ([-sys.float_info.max, sys.float_info.max], [-sys.float_info.max, sys.float_info.max], [-sys.float_info.max, sys.float_info.max]),
    ],
)
def test_williams_r_invalid_hlc_and_partial_window_overflow(high, low, close):
    """Even incomplete warmup validates overflowing per-bar and rolling spans."""
    with pytest.raises(itofin.ItofinError):
        williams_r(high, low, close)


def test_williams_r_invalid_period_and_nonnumeric_input():
    """Shared period validation and PyO3 argument conversion remain explicit."""
    with pytest.raises(itofin.ItofinError):
        williams_r([], [], [], period=0)
    with pytest.raises(OverflowError):
        williams_r([], [], [], period=-1)
    with pytest.raises(TypeError):
        williams_r(cast(Any, ["not a price"]), [0.0], [1.0])
