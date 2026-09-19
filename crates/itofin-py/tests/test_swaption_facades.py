"""Eonia/OIS cached QuantLib value and vanilla builder dates, ownership and errors."""

import gc
import json
import math
from pathlib import Path

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import Currency, Eonia, Euribor, OvernightIndex, SwapIndex
from itofin.instruments import EuropeanExercise, MakeOis, MakeSwaption, SettlementMethod, SettlementType, Swaption
from itofin.pricingengines import BlackSwaptionEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention as BDC
from itofin.time import Calendar, Date, DayCounter, Period

ORACLE = json.loads((Path(__file__).resolve().parents[3] / "sdk/go/testdata/makeswaption_oracle.json").read_text())


def _ois_option():
    settings = Settings()
    today = Date(13, 3, 2002)
    settings.set_evaluation_date(today)
    calendar = Calendar.target()
    settlement = calendar.advance(today, 2, "Days", BDC.Following, False)
    dc = DayCounter.actual365_fixed()
    discount = FlatForward(settlement, 0.05, dc)
    forward = FlatForward(settlement, 0.04, dc)
    index = Eonia(forward, settings)
    exercise = calendar.advance(settlement, 5, "Years", BDC.Following, False)
    start = calendar.advance(exercise, 2, "Days", BDC.Following, False)
    swap = MakeOis(
        Period(10, "Years"),
        index,
        settings,
        fixed_rate=0.06,
        effective_date=start,
        fixed_leg_day_count=DayCounter.thirty360_bond_basis(),
    ).build()
    option = Swaption.from_ois(
        swap, EuropeanExercise(exercise), SettlementType.Physical, SettlementMethod.PhysicalOTC, settings
    )
    quote = SimpleQuote(0.20)
    option.set_black_engine(BlackSwaptionEngine.with_flat_vol(discount, quote, dc, 0.0, settings))
    return option, swap, quote, settings


def test_eonia_ois_cached_value_retains_shared_underlying_and_reprices():
    """QuantLib test-suite/swaption.cpp testCachedValue OIS arm, absolute 1e-12."""
    option, swap, quote, settings = _ois_option()
    gc.collect()
    assert option.npv() == pytest.approx(0.014101075767, rel=0, abs=1e-12)
    assert swap.fixed_rate() == 0.06
    del swap
    gc.collect()
    assert option.npv() == pytest.approx(0.014101075767, rel=0, abs=1e-12)
    quote.set_value(0.30)
    assert not option.is_calculated()
    assert option.npv() > 0.014101075767
    quote.set_value(0.20)
    del quote
    gc.collect()
    assert option.npv() == pytest.approx(0.014101075767, rel=0, abs=1e-12)
    settings.set_evaluation_date(Date(16, 3, 2007))
    assert option.npv() == 0.0


def test_eonia_conventions_forecasts_lifetime_and_errors():
    """Zero lag and TARGET holidays distinguish the overnight conventions."""
    settings = Settings()
    today = Date(9, 10, 2015)
    settings.set_evaluation_date(today)
    index = Eonia(FlatForward(today, 0.04, DayCounter.actual360()), settings)
    assert isinstance(index, OvernightIndex)
    assert index.fixing_days() == 0
    assert index.currency().code() == "EUR"
    assert index.fixing_calendar().name == "TARGET"
    assert not index.fixing_calendar().is_business_day(Date(25, 12, 2015))
    expected = math.expm1(0.04 * 3 / 360) / (3 / 360)
    assert index.fixing(today, True) == pytest.approx(expected, rel=0, abs=1e-13)
    assert index.day_counter().year_fraction(today, Date(12, 10, 2015)) == 3 / 360
    empty = Eonia(None, settings)
    with pytest.raises(ItofinError):
        empty.fixing(Date(12, 10, 2015))
    with pytest.raises(ItofinError):
        index.fixing(Date(10, 10, 2015))
    with pytest.raises(ItofinError):
        index.fixing(Date(8, 10, 2015))
    del settings
    gc.collect()
    assert index.fixing(today, True) == pytest.approx(expected, rel=0, abs=1e-13)


def _swap_index(empty=False, dated=True):
    settings = Settings()
    today = Date(9, 10, 2015)
    if dated:
        settings.set_evaluation_date(today)
    curve = None if empty else FlatForward(today, 0.05, DayCounter.actual360())
    index = SwapIndex(
        "EuriborSwapIsdaFixA",
        Period(5, "Years"),
        2,
        Currency.eur(),
        Calendar.target(),
        Period(1, "Years"),
        BDC.ModifiedFollowing,
        DayCounter.thirty360_bond_basis(),
        Euribor(Period(6, "Months"), curve, settings),
        settings,
    )
    return index


def test_make_swaption_calendar_and_independent_atm_fixing_oracles():
    """QuantLib testMakeSwaptionWithExerciseCalendar and independent SwapIndex pins."""
    index = _swap_index()
    tenor = Period(1, "Years")
    maker = MakeSwaption(index, tenor)
    custom = MakeSwaption(index, tenor, exercise_calendar=Calendar.united_states("Settlement"))
    assert ORACLE["quantlib"] == "1.43"
    for row in ORACLE["fixings"]:
        year, month, day = map(int, row["date"].split("-"))
        assert index.fixing(Date(day, month, year)) == pytest.approx(row["fixing"], rel=0, abs=1e-12)
    del index
    gc.collect()
    default = maker.build()
    assert default.exercise_date() == Date(10, 10, 2016)
    assert default.underlying_fixed_rate() == pytest.approx(ORACLE["fixings"][0]["fixing"], rel=0, abs=1e-12)
    assert custom.build().exercise_date() == Date(11, 10, 2016)
    assert custom.build().underlying_fixed_rate() == pytest.approx(ORACLE["fixings"][1]["fixing"], rel=0, abs=1e-12)
    explicit = MakeSwaption(
        _swap_index(), fixing_date=Date(10, 10, 2016), strike=0.05, exercise_date=Date(11, 4, 2016), nominal=123
    ).build()
    assert explicit.exercise_date() == Date(11, 4, 2016)
    assert explicit.underlying_fixed_rate() == 0.05
    assert explicit.underlying_nominal() == 123
    with pytest.raises(ItofinError):
        default.npv()


def test_make_swaption_date_source_and_core_errors():
    """Ambiguous dates, missing evaluation/forward curve and invalid exercises fail."""
    index = _swap_index()
    for kwargs in [{}, {"option_tenor": Period(1, "Years"), "fixing_date": Date(10, 10, 2016)}]:
        with pytest.raises(ValueError, match="exactly one"):
            MakeSwaption(index, **kwargs)
    with pytest.raises(ItofinError, match="exercise date"):
        MakeSwaption(index, fixing_date=Date(10, 10, 2016), exercise_date=Date(11, 10, 2016)).build()
    with pytest.raises(ItofinError):
        MakeSwaption(_swap_index(empty=True), Period(1, "Years")).build()
    with pytest.raises(ItofinError):
        MakeSwaption(_swap_index(dated=False), Period(1, "Years"), strike=0.05).build()
    with pytest.raises(ItofinError):
        MakeSwaption(index, Period(1, "Years"), strike=0.05, indexed_coupons=True).build()
    assert (
        MakeSwaption(index, Period(1, "Years"), strike=0.05, indexed_coupons=False).build().underlying_fixed_rate()
        == 0.05
    )


def test_make_swaption_option_convention_override():
    """Month-end rolling distinguishes the default from an explicit Following override."""
    settings = Settings()
    settings.set_evaluation_date(Date(30, 9, 2016))
    curve = FlatForward(Date(30, 9, 2016), 0.05, DayCounter.actual360())
    index = SwapIndex(
        "EuriborSwapIsdaFixA",
        Period(5, "Years"),
        2,
        Currency.eur(),
        Calendar.target(),
        Period(1, "Years"),
        BDC.ModifiedFollowing,
        DayCounter.thirty360_bond_basis(),
        Euribor(Period(6, "Months"), curve, settings),
        settings,
    )
    default = MakeSwaption(index, Period(1, "Years"), strike=0.05).build()
    following = MakeSwaption(index, Period(1, "Years"), strike=0.05, option_convention=BDC.Following).build()
    assert default.exercise_date() == Date(29, 9, 2017)
    assert following.exercise_date() == Date(2, 10, 2017)
