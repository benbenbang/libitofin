"""SOFR future prices and custom pillars against independent QuantLib fixtures."""
import csv
import gc
import math
from datetime import date, timedelta
from pathlib import Path

import pytest
from itofin import Settings
from itofin.indexes import Sofr
from itofin.instruments import OvernightIndexFuture
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward, OvernightIndexFutureRateHelper, PiecewiseLogLinearDiscount, Pillar, RateAveraging, SofrFutureRateHelper
from itofin.time import Date, DayCounter, Frequency

FIXTURES = Path(__file__).resolve().parents[2] / "libitofin/tests/fixtures/sofr_futures"


def serial(value):
    d = date(1899, 12, 30) + timedelta(days=int(value))
    return Date(d.day, d.month, d.year)


def near(actual, expected):
    assert math.isfinite(actual) and abs(actual - expected) <= 1e-9


@pytest.mark.parametrize("row", list(csv.DictReader((FIXTURES / "accrual.csv").open())))
def test_overnight_future_quantlib_accrual(row):
    settings = Settings()
    settings.set_evaluation_date(serial(row["today"]))
    curve = FlatForward(Date(17, 6, 2024), .035, DayCounter.actual365_fixed())
    index = Sofr(curve, settings)
    today = int(row["today"])
    for day, value in [(18, .02), (20, .025), (21, .03), (24, .04)]:
        fixing_serial = (date(2024, 6, day) - date(1899, 12, 30)).days
        if fixing_serial < today or (row["today_fixing"] == "1" and fixing_serial == today):
            index.add_fixing(Date(day, 6, 2024), value)
    mode = RateAveraging.Simple if row["averaging"] == "0" else RateAveraging.Compound
    convexity = SimpleQuote(0.0)
    future = OvernightIndexFuture(index, serial(row["start"]), serial(row["end"]), convexity, mode)
    near(future.npv(), float(row["price"]))
    assert not future.is_expired()
    assert future.value_date() == serial(row["start"])
    assert future.maturity_date() == serial(row["end"])
    convexity.set_value(.001)
    near(future.npv(), float(row["price"]) - .1)
    near(future.convexity_adjustment(), .001)
    del index, curve
    gc.collect()
    convexity.set_value(.002)
    near(future.npv(), float(row["price"]) - .2)
    del convexity
    settings.set_evaluation_date(Date(1, 7, 2024))
    assert future.is_expired()
    near(future.npv(), 0.0)


def test_sofr_bootstrap_juneteenth_and_custom_pillars():
    settings = Settings()
    settings.set_evaluation_date(Date(27, 6, 2024))
    index = Sofr(None, settings)
    for day in [18, 20, 21, 24, 25, 26, 27]:
        index.add_fixing(Date(day, 6, 2024), .02)
    near(index.fixing(Date(18, 6, 2024)), .02)
    rows = [r for r in csv.DictReader((FIXTURES / "curves.csv").open()) if r["case"] == "juneteenth"]
    quotes = [SimpleQuote(float(r["quote"])) for r in rows]
    helpers = [SofrFutureRateHelper(q, int(r["month"]), int(r["year"]), Frequency.Quarterly, settings) for q, r in zip(quotes, rows)]
    curve = PiecewiseLogLinearDiscount(Date(27, 6, 2024), helpers, DayCounter.actual365_fixed())
    future = OvernightIndexFuture(Sofr(curve, settings), Date(19, 6, 2024), Date(18, 9, 2024))
    near(future.npv(), 97.220)
    quotes[0].set_value(97.1)
    near(future.npv(), 97.1)
    settings.set_evaluation_date(Date(15, 3, 2024))
    price = SimpleQuote(99)
    custom = Date(20, 4, 2024)
    helper = OvernightIndexFutureRateHelper(price, Date(20, 3, 2024), Date(20, 6, 2024), index, pillar=Pillar.CustomDate, custom_pillar_date=custom)
    assert helper.pillar_date() == custom
    helper = SofrFutureRateHelper(price, 6, 2024, Frequency.Quarterly, settings, pillar=Pillar.CustomDate, custom_pillar_date=Date(15, 7, 2024))
    assert helper.pillar_date() == Date(15, 7, 2024)
    for invalid in [None, Date(1, 3, 2024), Date(20, 7, 2024)]:
        with pytest.raises(Exception):
            OvernightIndexFutureRateHelper(price, Date(20, 3, 2024), Date(20, 6, 2024), index, pillar=Pillar.CustomDate, custom_pillar_date=invalid)
    with pytest.raises(Exception):
        SofrFutureRateHelper(price, 13, 2024, Frequency.Quarterly, settings)
    with pytest.raises(Exception):
        SofrFutureRateHelper(price, 6, 2024, Frequency.Annual, settings)
    with pytest.raises(Exception):
        index.add_fixing(Date(18, 6, 2024), float("nan"))
    with pytest.raises(Exception):
        index.add_fixing(Date(18, 6, 2024), .03)
    assert SofrFutureRateHelper(price, 6, 2024, Frequency.Quarterly, settings).pillar_date() == Date(18, 9, 2024)


@pytest.mark.parametrize("generic", [False, True])
def test_monthly_and_generic_overnight_helper_reprice(generic):
    settings = Settings()
    today = Date(27, 6, 2024)
    settings.set_evaluation_date(today)
    price = SimpleQuote(96.5)
    index = Sofr(None, settings)
    if generic:
        helper = OvernightIndexFutureRateHelper(price, Date(1, 7, 2024), Date(1, 8, 2024), index, averaging_method=RateAveraging.Simple, pillar=Pillar.CustomDate, custom_pillar_date=Date(15, 7, 2024))
    else:
        helper = SofrFutureRateHelper(price, 7, 2024, Frequency.Monthly, settings)
    curve = PiecewiseLogLinearDiscount(today, [helper], DayCounter.actual365_fixed())
    future = OvernightIndexFuture(Sofr(curve, settings), Date(1, 7, 2024), Date(1, 8, 2024), averaging_method=RateAveraging.Simple)
    near(future.npv(), 96.5)
    near(helper.implied_quote(), 96.5)
    del curve, index, helper
    gc.collect()
    price.set_value(97.0)
    near(future.npv(), 97.0)
