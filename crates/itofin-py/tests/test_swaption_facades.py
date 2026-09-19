"""Eonia/OIS cached QuantLib value, conventions, ownership and errors."""

import gc
import math

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import Eonia, OvernightIndex
from itofin.instruments import EuropeanExercise, MakeOis, SettlementMethod, SettlementType, Swaption
from itofin.pricingengines import BlackSwaptionEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention as BDC
from itofin.time import Calendar, Date, DayCounter, Period


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
