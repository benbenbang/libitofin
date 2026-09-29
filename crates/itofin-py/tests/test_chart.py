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


def test_taiwan_kd_seed_warmup_and_flat_range():
    """KD uses rolling RSV and recursive K/D lines seeded at 50."""
    values = chart.kd(
        [12.0, 14.0, 16.0, 18.0],
        [8.0, 10.0, 12.0, 14.0],
        [10.0, 12.0, 14.0, 16.0],
        period=3,
    )
    assert values.rsv.to_list() == [None, None, 75.0, 75.0]
    assert values.k.first_valid == values.d.first_valid == 2
    np.testing.assert_allclose(values.k.values[2:], [58.333333333333336, 63.88888888888889])
    np.testing.assert_allclose(values.d.values[2:], [52.77777777777778, 56.48148148148148])
    flat = chart.kd([2.0] * 9, [2.0] * 9, [2.0] * 9)
    assert flat.rsv.to_list()[-1] == 50.0
    assert flat.k.to_list()[-1] == 50.0
    assert flat.d.to_list()[-1] == 50.0
    assert flat.rsv.first_valid == flat.k.first_valid == flat.d.first_valid == 8
    assert chart.kd([], [], []).rsv.to_list() == []
    with pytest.raises(itofin.ItofinError):
        chart.kd([1.0], [], [0.5], period=1)
    with pytest.raises(itofin.ItofinError):
        chart.kd([1.0], [0.0], [2.0], period=1)
    with pytest.raises(itofin.ItofinError):
        chart.kd([1.0], [0.0], [0.5], period=0)
    with pytest.raises(itofin.ItofinError):
        chart.kd([float("nan")], [0.0], [0.5], period=1)


def test_macd_sma_seeded_signal_and_default_warmup():
    """The signal waits for valid line values and uses their first SMA."""
    values = chart.macd([1.0, 2.0, 3.0, 4.0], fast_period=2, slow_period=3, signal_period=2)
    assert values.line.to_list() == [None, None, 0.5, 0.5]
    assert values.signal.to_list() == [None, None, None, 0.5]
    assert values.histogram.to_list() == [None, None, None, 0.0]
    assert values.line.values.dtype == np.float64
    assert values.line.first_valid == 2
    assert values.signal.first_valid == values.histogram.first_valid == 3
    default = chart.macd(list(range(1, 35)))
    assert default.line.first_valid == 25
    assert default.signal.first_valid == default.histogram.first_valid == 33
    assert default.signal.to_list()[:33] == [None] * 33
    assert chart.macd([]).line.to_list() == []
    with pytest.raises(itofin.ItofinError):
        chart.macd([1.0], fast_period=2, slow_period=2, signal_period=1)
    with pytest.raises(itofin.ItofinError):
        chart.macd([1.0], fast_period=1, slow_period=2, signal_period=0)
    with pytest.raises(itofin.ItofinError):
        chart.macd([float("inf")])
