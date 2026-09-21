"""Custom helper pillars against the independent C++ QuantLib fixture."""

import gc
import json
import math
from pathlib import Path

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import CpiInterpolationType, Estr, Euribor, YoYInflationIndex, ZeroInflationIndex
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    FlatForward, FraRateHelper, OISRateHelper, PiecewiseLogLinearDiscount,
    PiecewiseYoYInflationCurve, PiecewiseZeroInflationCurve, Pillar,
    SwapRateHelper, YearOnYearInflationSwapHelper, ZeroCouponInflationSwapHelper,
)
from itofin.time import BusinessDayConvention as BDC
from itofin.time import Calendar, Date, DayCounter, Frequency, Period

ORACLE = json.loads((Path(__file__).parents[3] / "sdk/go/testdata/custom_pillars.json").read_text())
TODAY = Date(15, 6, 2026)


def date(text):
    year, month, day = map(int, text.split("-"))
    return Date(day, month, year)


def near(actual, expected):
    assert math.isfinite(actual)
    assert abs(actual - expected) <= 1e-12


def yield_factory(kind, settings, quote):
    ibor = Euribor(Period(3, "Months"), None, settings)
    if kind == 0:
        return lambda **kw: FraRateHelper(quote, Period(3, "Months"), ibor, **kw)
    if kind == 1:
        return lambda **kw: SwapRateHelper(quote, Period(1, "Years"), Calendar.target(), Frequency.Annual,
            BDC.ModifiedFollowing, DayCounter.thirty360_bond_basis(), ibor, **kw)
    overnight = Estr(None, settings)
    return lambda **kw: OISRateHelper(2, Period(1, "Years"), quote, overnight, 2, BDC.Following,
        Frequency.Annual, Period(0, "Days"), settings, **kw)


@pytest.mark.parametrize("row", ORACLE["yield"])
def test_custom_yield_pillars_quantlib_and_recovery(row):
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    quote = SimpleQuote(0.03)
    factory = yield_factory(row["kind"], settings, quote)
    for custom in [None, date(row["earliest"]) - 1, date(row["latest_relevant"]) + 1]:
        with pytest.raises(ItofinError):
            factory(pillar=Pillar.CustomDate, custom_pillar_date=custom)
    with pytest.raises(ItofinError):
        factory(custom_pillar_date=date(row["pillar"]))
    helper = factory(pillar=Pillar.CustomDate, custom_pillar_date=date(row["pillar"]))
    for key, method in [("earliest", helper.earliest_date), ("maturity", helper.maturity_date),
                        ("latest_relevant", helper.latest_relevant_date), ("pillar", helper.pillar_date),
                        ("latest", helper.latest_date)]:
        assert method() == date(row[key])
    curve = PiecewiseLogLinearDiscount(TODAY, [helper], DayCounter.actual365_fixed())
    del factory
    gc.collect()
    near(curve.discount_date(date(row["latest_relevant"]), True), row["discount"])
    near(helper.implied_quote(), 0.03)
    quote.set_value(0.031)
    near(curve.discount_date(date(row["latest_relevant"]), True), row["updated_discount"])
    near(helper.implied_quote(), 0.031)
    settings.set_evaluation_date(Date(15, 6, 2028))
    with pytest.raises(ItofinError):
        curve.discount_date(date(row["latest_relevant"]), True)
    settings.set_evaluation_date(TODAY)
    quote.set_value(0.03)
    del helper, quote
    gc.collect()
    near(curve.discount_date(date(row["latest_relevant"]), True), row["discount"])


def test_custom_fra_overloads_and_swap_discount():
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    index = Euribor(Period(3, "Months"), None, settings)
    quote = SimpleQuote(0.03)
    custom = Date(15, 10, 2026)
    for helper in [FraRateHelper.from_rate(0.03, Period(3, "Months"), index, pillar=Pillar.CustomDate, custom_pillar_date=custom),
                   FraRateHelper.from_months(quote, 3, index, pillar=Pillar.CustomDate, custom_pillar_date=custom),
                   FraRateHelper.from_dates(quote, Date(17, 9, 2026), Date(17, 12, 2026), index, pillar=Pillar.CustomDate, custom_pillar_date=custom)]:
        assert helper.pillar_date() == custom
    discount = FlatForward(TODAY, 0.025, DayCounter.actual365_fixed())
    helper = yield_factory(1, settings, quote)(discount=discount, pillar=Pillar.CustomDate,
        custom_pillar_date=Date(15, 3, 2027))
    assert helper.pillar_date() == Date(15, 3, 2027)


def inflation_market():
    settings = Settings()
    settings.set_evaluation_date(Date(13, 8, 2007))
    zero = ZeroInflationIndex.uk_rpi(settings)
    for month, value in enumerate([204.4, 205.4, 206.2, 207.3], 4):
        zero.add_fixing(Date(1, month, 2007), value)
    yoy = YoYInflationIndex("YY_RPI", "UK", "UK", False, Frequency.Monthly, Period(1, "Months"),
        "Pound", "GBP", 826, "£", "p", 100, settings)
    nominal = FlatForward(Date(13, 8, 2007), 0.05, DayCounter.actual360())
    return settings, zero, yoy, nominal


@pytest.mark.parametrize("row", ORACLE["inflation"])
def test_custom_inflation_pillars_and_nodes_quantlib(row):
    settings, zero, yoy, nominal = inflation_market()
    dc = DayCounter.thirty360_bond_basis()
    interpolation = CpiInterpolationType.Flat if row["flat"] else CpiInterpolationType.Linear
    prefix = (SimpleQuote(0.0295), Period(2, "Months"), Date(13, 8, 2008), Calendar.united_kingdom(), BDC.ModifiedFollowing, dc)
    factories = [lambda **kw: ZeroCouponInflationSwapHelper(*prefix, zero, interpolation, settings, **kw),
                 lambda **kw: YearOnYearInflationSwapHelper(*prefix, yoy, interpolation, nominal, settings, **kw)]
    for factory in factories:
        with pytest.raises(ItofinError):
            factory(pillar=Pillar.CustomDate)
        if not row["flat"]:
            for invalid in [Date(31, 5, 2008), Date(2, 7, 2008)]:
                with pytest.raises(ItofinError):
                    factory(pillar=Pillar.CustomDate, custom_pillar_date=invalid)
    zh, yh = [factory(pillar=Pillar.CustomDate, custom_pillar_date=Date(15, 6, 2008)) for factory in factories]
    assert zh.pillar_date() == date(row["pillar"])
    assert zh.latest_date() == date(row["latest"])
    assert yh.pillar_date() == date(row["yoy_pillar"])
    assert yh.latest_date() == date(row["yoy_latest"])
    zc = PiecewiseZeroInflationCurve(Date(13, 8, 2007), Date(1, 7, 2007), Frequency.Monthly, dc, [zh])
    yc = PiecewiseYoYInflationCurve(Date(13, 8, 2007), Date(1, 7, 2007), 0.0295, Frequency.Monthly, dc, [yh])
    del factories, zh, yh, prefix, zero, yoy, nominal
    gc.collect()
    near(zc.nodes()[-1][1], row["zero_rate"])
    near(yc.nodes()[-1][1], row["yoy_rate"])
