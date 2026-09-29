"""Shared chart fixtures matching sdk/go/chart_test.go."""

import numpy as np
import pytest

import itofin
from itofin import chart
from itofin.chart import sma


def test_chart_averages_and_warmup():
    assert sma is chart.sma
    close = [1.0, 2.0, 3.0, 4.0]
    for average in (chart.sma, chart.ema):
        result = average(close, 3)
        assert result.first_valid == 2
        assert result.values.dtype == np.float64
        assert result.values.tolist() == [0.0, 0.0, 2.0, 3.0]
        assert result.to_list() == [None, None, 2.0, 3.0]
    assert chart.sma([1.0, 2.0, 3.0, 10.0], 3).values[-1] == 5.0
    assert chart.ema([1.0, 2.0, 3.0, 10.0], 3).values[-1] == 6.0
    assert chart.sma([], 2).first_valid == 0
    assert chart.ema([1.0], 2).to_list() == [None]


def test_chart_volume_direction_and_errors():
    bars = chart.volume_bars(
        [1.0, 2.0, -2.0], [3.0, 3.0, 0.0], [0.0, 0.0, -3.0],
        [2.0, 1.0, -2.0], [10.0, 11.0, 0.0],
    )
    assert bars.volume.first_valid == 0
    assert bars.volume.values.tolist() == [10.0, 11.0, 0.0]
    assert bars.direction.dtype == np.int8
    assert bars.direction.tolist() == [1, -1, 0]
    empty = chart.volume_bars([], [], [], [], [])
    assert empty.volume.values.size == 0
    assert empty.direction.size == 0
    with pytest.raises(itofin.ItofinError):
        chart.sma([1.0], 0)
    with pytest.raises(itofin.ItofinError):
        chart.ema([float("nan")], 1)
    with pytest.raises(itofin.ItofinError):
        chart.volume_bars([1.0], [], [], [], [])
    with pytest.raises(itofin.ItofinError):
        chart.volume_bars([1.0], [2.0], [0.0], [1.0], [-1.0])
