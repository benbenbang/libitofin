"""Shared chart fixtures matching sdk/go/chart_test.go."""

import numpy as np
import pytest

import itofin
from itofin import chart
from itofin.chart import sma


def test_chart_averages_and_warmup():
    """Averages preserve alignment and distinguish warmup from valid zero."""
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
    """Volume returns validated values and close-versus-open direction."""
    bars = chart.volume_bars(
        [1.0, 2.0, -2.0],
        [3.0, 3.0, 0.0],
        [0.0, 0.0, -3.0],
        [2.0, 1.0, -2.0],
        [10.0, 11.0, 0.0],
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


def test_bollinger_population_bands_and_warmup():
    """Bands use population variance and expose the same warmup on all lines."""
    bands = chart.bollinger_bands([1.0, 2.0, 3.0, 4.0], period=3)
    offset = 2.0 * np.sqrt(2.0 / 3.0)
    assert bands.middle.to_list() == [None, None, 2.0, 3.0]
    assert bands.upper.to_list()[:2] == [None, None]
    assert bands.lower.to_list()[:2] == [None, None]
    np.testing.assert_allclose(bands.upper.values[2:], [2.0 + offset, 3.0 + offset])
    np.testing.assert_allclose(bands.lower.values[2:], [2.0 - offset, 3.0 - offset])
    assert bands.middle.first_valid == bands.upper.first_valid == bands.lower.first_valid == 2
    flat = chart.bollinger_bands([1.0] * 20)
    assert flat.middle.to_list()[-1] == 1.0
    assert flat.upper.to_list()[-1] == 1.0
    assert flat.lower.to_list()[-1] == 1.0
    assert chart.bollinger_bands([], 3).middle.to_list() == []
    with pytest.raises(itofin.ItofinError):
        chart.bollinger_bands([1.0], period=0)
    with pytest.raises(itofin.ItofinError):
        chart.bollinger_bands([1.0], period=1, multiplier=-1.0)
    with pytest.raises(itofin.ItofinError):
        chart.bollinger_bands([1.0], period=1, multiplier=float("inf"))
    with pytest.raises(itofin.ItofinError):
        chart.bollinger_bands([float("nan")], period=1)


def test_wilder_rsi_warmup_flat_and_invalid():
    """RSI uses Wilder smoothing and treats flat prices as neutral."""
    result = chart.rsi([1.0, 2.0, 3.0, 2.0, 2.0], period=2)
    assert result.first_valid == 2
    assert result.values.dtype == np.float64
    assert result.to_list() == [None, None, 100.0, 50.0, 50.0]
    assert chart.rsi([2.0, 2.0, 2.0], period=2).to_list() == [None, None, 50.0]
    assert chart.rsi([3.0, 2.0, 1.0], period=2).to_list() == [None, None, 0.0]
    assert chart.rsi(list(range(15))).first_valid == 14
    assert chart.rsi([1.0], period=2).to_list() == [None]
    with pytest.raises(itofin.ItofinError):
        chart.rsi([1.0], period=0)
    with pytest.raises(itofin.ItofinError):
        chart.rsi([float("nan")], period=1)
