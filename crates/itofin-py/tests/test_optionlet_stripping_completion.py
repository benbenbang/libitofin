"""QuantLib 1.43 independent normal/nonflat prices and optionlet lifecycle gates."""

import gc

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import Euribor, Eonia
from itofin.instruments import CapFloor, CapFloorType
from itofin.pricingengines import BachelierCapFloorEngine, BlackCapFloorEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    CapFloorTermVolCurve, CapFloorTermVolSurface, FlatForward, OptionletStripper1,
    OptionletStripper2, StrippedOptionletAdapter, VolatilityType,
)
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Period, Schedule, Frequency

TENORS = [Period(n, "Years") for n in (1, 2, 3, 5, 7, 10)]
STRIKES = [.01, .02, .04, .06, .1]


def market(normal=True):
    settings = Settings()
    settings.set_evaluation_date(Date(28, 10, 2013))
    dc = DayCounter.actual365_fixed()
    curve = FlatForward(Date(28, 10, 2013), .04, dc)
    index = Euribor.six_months(curve, settings)
    quotes = [[SimpleQuote(.008 + .0002 * i + .002 * k if normal else .18)
               for k in STRIKES] for i in range(len(TENORS))]
    surface = CapFloorTermVolSurface.moving_with_quotes(
        0, Calendar.target(), BusinessDayConvention.Following, TENORS, STRIKES,
        quotes, dc, settings,
    )
    kind = VolatilityType.Normal if normal else VolatilityType.ShiftedLognormal
    stripper = OptionletStripper1(surface, index, kind, accuracy=1e-10)
    return settings, dc, curve, index, quotes, surface, stripper


def test_normal_nonflat_ql_prices_and_smile_snapshots():
    settings, _, curve, index, _, _, stripper = market()
    adapter = StrippedOptionletAdapter(stripper, settings)
    adapter.enable_extrapolation()
    assert stripper.switch_strike() == pytest.approx(.0398495071682556, abs=1e-12, rel=0)
    engine = BachelierCapFloorEngine(adapter, curve)
    for years, strike, expected in [(1, .02, .009692958950646112),
                                    (3, .04, .009169836497239313),
                                    (10, .06, .012808348961891588)]:
        cap = CapFloor(CapFloorType.Cap, Period(years, "Years"), index, strike,
                       Period(0, "Days"), settings)
        cap.set_bachelier_engine(engine)
        assert cap.npv() == pytest.approx(expected, abs=2.5e-8, rel=0)
    smile = adapter.smile_section(4.)
    assert smile.volatility(.02) == pytest.approx(.008713771862127703, abs=1e-10, rel=0)
    assert smile.variance(.02) == pytest.approx(smile.volatility(.02)**2 * 4, abs=1e-15, rel=0)
    assert smile.volatility(.035) == pytest.approx(.0088255669084960128, abs=1e-10, rel=0)
    for invalid in (float("nan"), float("inf"), -float("inf")):
        with pytest.raises(ItofinError):
            smile.volatility(invalid)
        with pytest.raises(ItofinError):
            smile.variance(invalid)
    assert smile.volatility(.02) == pytest.approx(.008713771862127703, abs=1e-10, rel=0)
    assert smile.exercise_time() == 4
    date = Date(28, 10, 2017)
    by_date = adapter.smile_section_date(date)
    by_tenor = adapter.smile_section_tenor(Period(4, "Years"))
    assert by_date.volatility(.04) > 0
    assert by_tenor.volatility(.04) == pytest.approx(adapter.volatility(Period(4, "Years"), .04), abs=1e-12, rel=0)
    del adapter, engine, stripper
    gc.collect()
    assert smile.volatility(.02) == pytest.approx(.008713771862127703, abs=1e-10, rel=0)


def test_switch_options_quote_errors_and_recovery():
    settings, _, _, index, quotes, surface, floating = market()
    explicit = OptionletStripper1(surface, index, VolatilityType.Normal, switch_strike=.03)
    assert explicit.switch_strike() == .03
    before = floating.switch_strike()
    quotes[0][0].set_value(float("nan"))
    for obj in (floating, explicit):
        with pytest.raises(ItofinError):
            obj.atm_optionlet_rates()
    quotes[0][0].set_value(.00802)
    assert floating.switch_strike() == pytest.approx(before, abs=1e-12, rel=0)
    adapter = StrippedOptionletAdapter(floating, settings)
    original = adapter.volatility(Period(3, "Years"), .04)
    quotes[2][2].set_value(.012)
    assert adapter.volatility(Period(3, "Years"), .04) != original
    quotes[2][2].set_value(.00848)
    assert adapter.volatility(Period(3, "Years"), .04) == pytest.approx(original, abs=1e-12, rel=0)


def test_stripper2_atm_curve_retention_updates_and_normal_rejection():
    settings, dc, curve, index, _, _, first = market(False)
    tenors = [Period(n, "Years") for n in (1, 3, 5)]
    quotes = [SimpleQuote(.20) for _ in tenors]
    atm = CapFloorTermVolCurve.moving(0, Calendar.target(), BusinessDayConvention.Following,
                                     tenors, quotes, dc, settings)
    fixed = CapFloorTermVolCurve(Date(28, 10, 2013), Calendar.target(),
                                BusinessDayConvention.Following, tenors, quotes, dc)
    assert atm.option_times() == fixed.option_times()
    assert fixed.volatility(tenors[1]) == pytest.approx(.2, abs=1e-14, rel=0)
    second = OptionletStripper2(first, atm)
    adapter = StrippedOptionletAdapter(second, settings)
    adapter.enable_extrapolation()
    engine = BlackCapFloorEngine(adapter, curve, None)
    strikes, prices = second.atm_cap_floor_strikes(), second.atm_cap_floor_prices()
    assert strikes == pytest.approx([.039850314101546484, .039850293421813322, .039849613061160541], abs=1e-12, rel=0)
    assert prices == pytest.approx([.0010954230960266622, .0087581286819643916, .019163763351720011], abs=2.5e-8, rel=0)
    assert second.spreads_vol() == pytest.approx([.020000005236476808, .020000058319575627, .020000166175862723], abs=1e-7, rel=0)
    for tenor, strike, target in zip(tenors, strikes, prices, strict=True):
        cap = CapFloor(CapFloorType.Cap, tenor, index, strike, Period(0, "Days"), settings)
        cap.set_black_engine(engine)
        assert cap.npv() == pytest.approx(target, abs=2.5e-8, rel=0)
    quotes[1].set_value(.21)
    assert second.atm_cap_floor_prices()[1] > prices[1]
    quotes[1].set_value(.20)
    assert second.atm_cap_floor_prices() == pytest.approx(prices, abs=1e-12, rel=0)
    _, _, _, _, _, _, normal = market()
    with pytest.raises(ItofinError):
        OptionletStripper2(normal, atm)
    del first, second, atm, quotes, fixed
    gc.collect()
    assert adapter.volatility(tenors[1], strikes[1]) > 0


def test_dont_throw_is_limited_to_implied_vol_failures():
    settings, _, _, index, quotes, surface, _ = market(False)
    strict = OptionletStripper1(surface, index, VolatilityType.ShiftedLognormal,
                               accuracy=1e-16, max_iter=1)
    with pytest.raises(ItofinError):
        strict.switch_strike()
    relaxed = OptionletStripper1(surface, index, VolatilityType.ShiftedLognormal,
                                accuracy=1e-16, max_iter=1, dont_throw=True)
    assert relaxed.switch_strike() > 0
    adapter = StrippedOptionletAdapter(relaxed, settings)
    assert adapter.volatility(Period(3, "Years"), .04) >= 0
    quotes[0][0].set_value(float("nan"))
    with pytest.raises(ItofinError):
        relaxed.switch_strike()
    quotes[0][0].set_value(.18)
    assert relaxed.switch_strike() > 0


def test_overnight_constructor_requires_frequency_and_retains_index():
    settings, _, curve, _, _, surface, _ = market()
    overnight = Eonia(curve, settings)
    stripper = OptionletStripper1.overnight(surface, overnight, VolatilityType.Normal,
                                           Period(6, "Months"), accuracy=1e-10)
    adapter = StrippedOptionletAdapter(stripper, settings)
    before = adapter.volatility(Period(3, "Years"), .04)
    assert before > 0
    with pytest.raises(ItofinError):
        OptionletStripper1.overnight(surface, overnight, VolatilityType.Normal, Period(0, "Months"))
    del overnight, stripper, surface
    gc.collect()
    assert adapter.volatility(Period(3, "Years"), .04) == before


def test_overnight_cap_constructor_matches_independent_ql_price():
    settings, dc, curve, _, _, surface, _ = market()
    index = Eonia(curve, settings)
    schedule = Schedule(Date(28, 10, 2014), Date(28, 10, 2016), Frequency.Semiannual,
                        Calendar.target(), BusinessDayConvention.ModifiedFollowing,
                        termination_convention=BusinessDayConvention.ModifiedFollowing)
    cap = CapFloor.overnight(CapFloorType.Cap, schedule, index, [.04], [], settings,
                            payment_lag=2, payment_adjustment=BusinessDayConvention.ModifiedFollowing)
    engine = BachelierCapFloorEngine.with_flat_vol(curve, SimpleQuote(.008), dc, settings)
    cap.set_bachelier_engine(engine)
    assert cap.coupon_count() == 4
    assert cap.npv() == pytest.approx(.008645761084412935, abs=2.5e-8, rel=0)
    stripped = OptionletStripper1.overnight(surface, index, VolatilityType.Normal,
                                            Period(6, "Months"), accuracy=1e-10)
    adapter = StrippedOptionletAdapter(stripped, settings)
    adapter.enable_extrapolation()
    stripped_engine = BachelierCapFloorEngine(adapter, curve)
    stripped_cap = CapFloor.overnight(CapFloorType.Cap, schedule, index, [.04], [], settings,
                                     payment_lag=2, payment_adjustment=BusinessDayConvention.ModifiedFollowing)
    stripped_cap.set_bachelier_engine(stripped_engine)
    assert stripped_cap.npv() == pytest.approx(1.542865396979454, abs=2.5e-8, rel=0)
    del index, schedule
    gc.collect()
    assert cap.npv() == pytest.approx(.008645761084412935, abs=2.5e-8, rel=0)


def test_retained_cap_observes_quotes_dates_and_discount_failure_recovery():
    settings, dc, curve, index, quotes, surface, _ = market()
    discount_quote = SimpleQuote(.04)
    discount = FlatForward.from_quote(Date(28, 10, 2013), discount_quote, dc)
    stripper = OptionletStripper1(surface, index, VolatilityType.Normal,
                                 accuracy=1e-10, discount=discount, dont_throw=True)
    adapter = StrippedOptionletAdapter(stripper, settings)
    engine = BachelierCapFloorEngine(adapter, discount)
    cap = CapFloor(CapFloorType.Cap, Period(3, "Years"), index, .04, Period(0, "Days"), settings)
    cap.set_bachelier_engine(engine)
    before = cap.npv()
    for i, row in enumerate(quotes):
        for quote, strike in zip(row, STRIKES, strict=True):
            quote.set_value(.008 + .0002 * i + .002 * strike + .001)
    assert cap.npv() > before
    for i, row in enumerate(quotes):
        for quote, strike in zip(row, STRIKES, strict=True):
            quote.set_value(.008 + .0002 * i + .002 * strike)
    assert cap.npv() == pytest.approx(before, abs=2.5e-8, rel=0)
    late = Date(15, 5, 2023)
    with pytest.raises(ItofinError):
        adapter.volatility_date(late, .04)
    settings.set_evaluation_date(Date(28, 12, 2013))
    assert adapter.volatility_date(late, .04) > 0
    settings.set_evaluation_date(Date(28, 10, 2013))
    with pytest.raises(ItofinError):
        adapter.volatility_date(late, .04)
    assert cap.npv() == pytest.approx(before, abs=2.5e-8, rel=0)
    discount_quote.set_value(float("nan"))
    with pytest.raises(ItofinError):
        stripper.atm_optionlet_rates()
    with pytest.raises(ItofinError):
        cap.npv()
    discount_quote.set_value(.04)
    assert cap.npv() == pytest.approx(before, abs=2.5e-8, rel=0)


@pytest.mark.parametrize("invalid", [float("nan"), float("inf"), -float("inf")])
def test_overnight_constructor_rejects_nonfinite_inputs_and_recovers(invalid):
    settings, dc, curve, _, _, _, _ = market()
    index = Eonia(curve, settings)
    schedule = Schedule(Date(28, 10, 2014), Date(28, 10, 2016), Frequency.Semiannual,
                        Calendar.target(), BusinessDayConvention.ModifiedFollowing,
                        termination_convention=BusinessDayConvention.ModifiedFollowing)
    for kind, caps, floors, nominal in (
        (CapFloorType.Cap, [.04], [], invalid),
        (CapFloorType.Cap, [invalid], [], 1.),
        (CapFloorType.Floor, [], [invalid], 1.),
    ):
        with pytest.raises(ItofinError, match="finite"):
            CapFloor.overnight(kind, schedule, index, caps, floors, settings, nominal)
    cap = CapFloor.overnight(CapFloorType.Cap, schedule, index, [.04], [], settings,
                            payment_lag=2, payment_adjustment=BusinessDayConvention.ModifiedFollowing)
    cap.set_bachelier_engine(BachelierCapFloorEngine.with_flat_vol(curve, SimpleQuote(.008), dc, settings))
    assert cap.npv() == pytest.approx(.008645761084412935, abs=2.5e-8, rel=0)


def test_overnight_constructor_maps_payment_date_overflow_to_error():
    settings, _, curve, _, _, _, _ = market()
    index = Eonia(curve, settings)
    schedule = Schedule(Date(28, 10, 2014), Date(28, 10, 2016), Frequency.Semiannual,
                        Calendar.target(), BusinessDayConvention.ModifiedFollowing,
                        termination_convention=BusinessDayConvention.ModifiedFollowing)
    for lag in (2**31 - 1, -2**31):
        with pytest.raises(ItofinError):
            CapFloor.overnight(CapFloorType.Cap, schedule, index, [.04], [], settings,
                              payment_lag=lag)
    cap = CapFloor.overnight(CapFloorType.Cap, schedule, index, [.04], [], settings)
    assert cap.coupon_count() == 4
