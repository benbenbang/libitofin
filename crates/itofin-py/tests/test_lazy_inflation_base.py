"""QuantLib lazy-base oracle through the concrete index-backed constructor."""

import csv
import gc
from pathlib import Path

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import CpiInterpolationType, ZeroInflationIndex
from itofin.quotes import SimpleQuote
from itofin.termstructures import PiecewiseZeroInflationCurve, ZeroCouponInflationSwapHelper
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Period

FIXTURES = Path(__file__).resolve().parents[2] / "libitofin/tests/fixtures/lazy_inflation_base/oracle.csv"
RATES = [2.93, 2.95, 2.965, 2.98, 3.0, 3.06, 3.175, 3.243, 3.293, 3.338, 3.348, 3.348, 3.308, 3.228]
MATURITIES = [
    (13, 2008),
    (13, 2009),
    (13, 2010),
    (15, 2011),
    (13, 2012),
    (13, 2014),
    (13, 2017),
    (13, 2019),
    (15, 2022),
    (14, 2027),
    (13, 2032),
    (15, 2037),
    (13, 2047),
    (13, 2057),
]
FIXINGS = [
    189.9,
    189.9,
    189.6,
    190.5,
    191.6,
    192.0,
    192.2,
    192.2,
    192.6,
    193.1,
    193.3,
    193.6,
    194.1,
    193.4,
    194.2,
    195.0,
    196.5,
    197.7,
    198.5,
    198.5,
    199.2,
    200.1,
    200.4,
    201.1,
    202.7,
    201.6,
    203.1,
    204.4,
    205.4,
    206.2,
    207.3,
]


@pytest.mark.parametrize("corrected", [False, True])
def test_lazy_base_nodes_and_forecasts_follow_published_fixings(corrected):
    today = Date(13, 8, 2007)
    settings = Settings()
    settings.set_evaluation_date(today)
    index = ZeroInflationIndex.uk_rpi(settings)
    dc = DayCounter.thirty360_bond_basis()
    quotes = [SimpleQuote(0.0) for _ in RATES]
    helpers = [
        ZeroCouponInflationSwapHelper(
            quote,
            Period(3, "Months"),
            Date(day, 8, year),
            Calendar.united_kingdom(),
            BusinessDayConvention.ModifiedFollowing,
            dc,
            index,
            CpiInterpolationType.Flat,
            settings,
        )
        for quote, (day, year) in zip(quotes, MATURITIES)
    ]
    curve = PiecewiseZeroInflationCurve.with_last_fixing_date(today, index, Frequency.Monthly, dc, helpers)
    with pytest.raises(ItofinError):
        curve.base_date()
    for quote, rate in zip(quotes, RATES):
        quote.set_value(rate / 100)
    for i, value in enumerate(FIXINGS):
        index.add_fixing(Date(1, i % 12 + 1, 2005 + i // 12), 208.1 if corrected and i == 30 else value)
    index.link_to(curve)
    with FIXTURES.open() as stream:
        rows = list(csv.DictReader(stream))
    for phase in [1, 2, 3, 4] if corrected else [0]:
        if phase == 2:
            settings.set_evaluation_date(Date(13, 9, 2007))
            index.add_fixing(Date(1, 8, 2007), 208.4)
        if phase == 3:
            settings.set_evaluation_date(Date(13, 10, 2007))
        if phase == 4:
            quotes[0].set_value(RATES[0] / 100 + 0.0005)
        expected = [row for row in rows if int(row["phase"]) == phase]
        nodes = curve.nodes()
        assert len(nodes) == len(expected) == 15
        assert curve.base_date() == nodes[0][0]
        for (date, rate), row in zip(nodes, expected):
            assert date == Date(1, 1, 1901) + (int(row["date"]) - 367)
            assert rate == pytest.approx(float(row["rate"]), rel=0, abs=1e-12)
        assert index.fixing(Date(1, 8, 2012)) == pytest.approx(float(expected[0]["forecast"]), rel=0, abs=1e-7)
    del index, helpers, quotes, quote, settings
    gc.collect()
    assert curve.nodes() == nodes
