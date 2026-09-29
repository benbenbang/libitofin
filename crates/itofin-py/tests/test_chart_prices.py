"""Dated OHLC price series share the Rust core's validation and ordering."""

import pytest

import itofin
from itofin import chart
from itofin.time import Date


def test_chart_interval_prices_sorted_and_last_write_wins():
    """Bars sort by date, overwrite duplicates, and retain negative prices."""
    first = Date(1, 1, 2025)
    second = Date(2, 1, 2025)
    bars = chart.interval_prices(
        [second, first, second],
        [2.0, -2.0, 3.0],
        [3.0, -1.0, 4.0],
        [1.0, -3.0, 2.0],
        [2.5, -2.5, 3.5],
    )
    assert len(bars) == 2
    assert [bar.date for bar in bars] == [first, second]
    assert (bars[0].open, bars[0].high, bars[0].low, bars[0].close) == (-2.0, -1.0, -3.0, -2.5)
    assert (bars[1].open, bars[1].high, bars[1].low, bars[1].close) == (3.0, 4.0, 2.0, 3.5)
    with pytest.raises(AttributeError):
        setattr(bars[0], "close", 0.0)


def test_chart_interval_prices_empty_and_invalid():
    """The core rejects lengths, bad bounds, and nonfinite prices."""
    assert chart.interval_prices([], [], [], [], []) == []
    date = Date(1, 1, 2025)
    with pytest.raises(itofin.ItofinError):
        chart.interval_prices([date], [], [], [], [])
    with pytest.raises(itofin.ItofinError):
        chart.interval_prices([date], [2.0], [1.0], [0.0], [1.5])
    with pytest.raises(itofin.ItofinError):
        chart.interval_prices([date], [1.0], [2.0], [0.0], [float("nan")])
    with pytest.raises(itofin.ItofinError):
        chart.interval_prices([date], [1.0], [2.0], [float("inf")], [1.5])
