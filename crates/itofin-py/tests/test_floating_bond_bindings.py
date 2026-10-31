"""Floating bond controls, retained coupon/pricer lifetimes and native geometry."""

# standard library
import datetime
import gc
import json
import math
from pathlib import Path

# third party
import pytest

# itofin library
from itofin import ItofinError, Settings
from itofin.cashflows import BlackIborCouponPricer, npv
from itofin.indexes import Currency, CustomIborIndex, IborIndex
from itofin.instruments import Bond, FloatingRateBond
from itofin.pricingengines import DiscountingBondEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import ConstantOptionletVolatility, FlatForward, VolatilityType
from itofin.time import BusinessDayConvention as BDC, Calendar, Date, DayCounter, Frequency, Period, Schedule

ROWS = json.loads((Path(__file__).parent / "fixtures/floating_bindings_native.json").read_text())["rows"]


def _serial(value):
    return (datetime.date(value.year, value.month, value.day) - datetime.date(1899, 12, 30)).days


def _market(settings, calendar=None):
    calendar = calendar or Calendar.null_calendar()
    quote = SimpleQuote(0.035)
    forwarding = FlatForward.from_quote(Date(5, 1, 2024), quote, DayCounter.actual360())
    discount = FlatForward(Date(5, 1, 2024), 0.025, DayCounter.actual360())
    index = IborIndex("py-floating", Period(3, "Months"), 2, Currency.eur(), calendar, BDC.Unadjusted, False, DayCounter.actual360(), forwarding, settings)
    return index, discount, quote


def _bond(settings, index, start=None, end=None, calendar=None, **options):
    schedule = Schedule(start or Date(15, 1, 2024), end or Date(15, 1, 2027), Frequency.Quarterly, calendar or Calendar.null_calendar(), BDC.Unadjusted)
    return FloatingRateBond(2, 1000.0, schedule, index, DayCounter.actual360(), settings, **options)


def _vol(settings=None, quote=None, settlement_days=0, calendar=None):
    calendar = calendar or Calendar.null_calendar()
    if settings is not None:
        return ConstantOptionletVolatility.moving_with_quote(settlement_days, calendar, BDC.Unadjusted, quote or SimpleQuote(0.2), DayCounter.actual360(), VolatilityType.ShiftedLognormal, settings)
    return ConstantOptionletVolatility.with_quote(Date(5, 1, 2024), calendar, BDC.Unadjusted, quote or SimpleQuote(0.2), DayCounter.actual360(), VolatilityType.ShiftedLognormal)


@pytest.mark.parametrize("row", ROWS, ids=[row["name"] for row in ROWS])
def test_native_controls_and_complete_cashflows(row):
    """Match twelve native scenarios including mixed absent caps and a stub mode switch."""
    settings = Settings()
    settings.set_evaluation_date(Date(*row["evaluation"]))
    settings.set_using_at_par_coupons(row["at_par"])
    index, discount, _ = _market(settings)
    options = {name: row[name] for name in ["fixing_days", "gearings", "spreads", "caps", "floors"]}
    if row["ex_days"] is not None:
        options.update(ex_coupon_period=Period(row["ex_days"], "Days"), ex_coupon_calendar=Calendar.null_calendar())
    bond = _bond(settings, index, start=Date(*row["start"]), end=Date(*row["end"]), **options)
    for fixing in row["histories"]:
        index.add_fixing(Date(*fixing), 0.0387)
    if row["caps"] or row["floors"]:
        vol = None if row["volatility"] is None else ConstantOptionletVolatility(Date(5, 1, 2024), Calendar.null_calendar(), BDC.Unadjusted, row["volatility"], DayCounter.actual360(), VolatilityType.Normal if row["normal"] else VolatilityType.ShiftedLognormal)
        bond.set_coupon_pricer(BlackIborCouponPricer(vol))
    bond.set_engine(DiscountingBondEngine(discount, settings))
    assert isinstance(bond, Bond)
    values = [bond.npv(), bond.settlement_value(), bond.clean_price(), bond.dirty_price(), bond.accrued_amount(), bond.notional()]
    assert values == pytest.approx(row["values"], abs=3e-10, rel=3e-13)
    flows = bond.cashflows()
    assert len(flows) == len(row["flows"])
    for i, expected in enumerate(row["flows"]):
        assert _serial(flows[i].date()) == expected[0]
        assert flows[i].amount() == pytest.approx(expected[1], abs=3e-11, rel=3e-13)
    assert bond.index().tenor() == index.tenor()


def test_construct_before_history_and_pricer_then_recover():
    """Construction does not demand a past fixing or an optionlet pricer."""
    settings = Settings()
    settings.set_evaluation_date(Date(16, 1, 2024))
    index, discount, _ = _market(settings)
    bond = _bond(settings, index, caps=[0.04])
    bond.set_engine(DiscountingBondEngine(discount, settings))
    leg = bond.cashflows()
    flow = leg[0]
    with pytest.raises(ItofinError):
        flow.amount()
    index.add_fixing(Date(13, 1, 2024), 0.0387)
    bond.set_coupon_pricer(BlackIborCouponPricer())
    assert flow.amount() > 0.0
    with pytest.raises(ItofinError, match="volatility"):
        bond.npv()
    bond.set_coupon_pricer(BlackIborCouponPricer(_vol()))
    assert bond.npv() > 0.0


def test_shared_old_views_see_pricer_replacement_quotes_and_retained_owners():
    """Old flow views retain live replacement metadata without retaining the Bond."""
    settings = Settings()
    settings.set_evaluation_date(Date(5, 1, 2024))
    index, discount, quote = _market(settings)
    bond = _bond(settings, index, caps=[0.04])
    bond.set_engine(DiscountingBondEngine(discount, settings))
    leg, flow, retained_index = bond.cashflows(), bond.cashflows()[0], bond.index()
    first_pricer = BlackIborCouponPricer(_vol())
    bond.set_coupon_pricer(first_pricer)
    first = flow.amount()
    second_quote = SimpleQuote(0.8)
    second_pricer = BlackIborCouponPricer(_vol(settings, second_quote))
    bond.set_coupon_pricer(second_pricer)
    assert flow.amount() < first
    del first_pricer, second_pricer, index, discount, bond
    gc.collect()
    second_quote.set_value(0.1)
    assert flow.amount() > first
    quote.set_value(0.06)
    assert leg[0].amount() == flow.amount()
    assert retained_index.tenor() == Period(3, "Months")
    settings.set_evaluation_date(Date(31, 12, 2199))
    with pytest.raises(ItofinError):
        flow.amount()


def test_silent_at_par_policy_refresh_honors_native_freeze():
    """Changing the coupon forecasting policy refreshes only unfrozen valuations."""
    settings = Settings()
    settings.set_evaluation_date(Date(5, 1, 2024))
    index, discount, _ = _market(settings)
    bond = _bond(settings, index, start=Date(16, 2, 2024))
    bond.set_engine(DiscountingBondEngine(discount, settings))
    initial = bond.npv()
    settings.set_using_at_par_coupons(False)
    indexed = bond.npv()
    assert indexed != initial
    bond.freeze()
    settings.set_using_at_par_coupons(True)
    assert bond.npv() == indexed
    bond.recalculate()
    assert bond.npv() == initial
    settings.set_using_at_par_coupons(False)
    assert bond.npv() == initial
    bond.unfreeze()
    assert bond.npv() == indexed


def test_all_absent_option_vectors_require_explicit_pricer_and_zero_gearing_needs_none():
    """Distinguish absent vector entries from empty vectors and pure fixed coupons."""
    settings = Settings()
    settings.set_evaluation_date(Date(5, 1, 2024))
    index, discount, _ = _market(settings)
    bond = _bond(settings, index, caps=[None], floors=[None])
    with pytest.raises(ItofinError, match="pricer"):
        bond.cashflows()[0].amount()
    bond.set_coupon_pricer(BlackIborCouponPricer())
    assert bond.cashflows()[0].amount() > 0.0
    near_min = _bond(settings, index, start=Date(1, 1, 1901), end=Date(1, 1, 1902), gearings=[0.0], spreads=[0.03])
    assert near_min.cashflows()[0].amount() > 0.0
    near_min.set_engine(DiscountingBondEngine(discount, settings))
    assert near_min.npv() == 0.0


def test_settings_identity_boundaries_and_original_pricer_survive_rejected_replacement():
    """Reject incompatible settings and unsafe dates without altering a valid pricer."""
    settings = Settings()
    settings.set_evaluation_date(Date(5, 1, 2024))
    index, _, _ = _market(settings)
    with pytest.raises(ItofinError, match="share settings"):
        _bond(Settings(), index)
    bond = _bond(settings, index, caps=[0.04])
    bond.set_coupon_pricer(BlackIborCouponPricer(_vol()))
    flow = bond.cashflows()[0]
    initial = flow.amount()
    wrong = Settings()
    wrong.set_evaluation_date(Date(5, 1, 2024))
    with pytest.raises(ItofinError, match="share settings"):
        bond.set_coupon_pricer(BlackIborCouponPricer(_vol(wrong)))
    assert flow.amount() == initial
    with pytest.raises(ItofinError):
        _bond(settings, index, start=Date(1, 1, 1901), end=Date(1, 1, 1902))
    with pytest.raises(ItofinError):
        _bond(settings, index, fixing_days=2**32-1)
    with pytest.raises(ItofinError):
        _bond(settings, index, caps=[math.nan])
    with pytest.raises(ItofinError):
        _bond(settings, index, gearings=[1.0]*20)
    bond.set_coupon_pricer(BlackIborCouponPricer(_vol(settings, settlement_days=2**32-1)))
    with pytest.raises(ItofinError):
        flow.amount()


@pytest.mark.parametrize("with_quote", [False, True])
def test_moving_surface_scalar_and_quote_guards_survive_old_view_replacement(with_quote):
    """Both moving constructors retain checked reference-date metadata for old views."""
    settings = Settings()
    settings.set_evaluation_date(Date(5, 1, 2024))
    index, _, _ = _market(settings)
    bond = _bond(settings, index, caps=[0.04])
    view = bond.cashflows()[0]
    if with_quote:
        surface = ConstantOptionletVolatility.moving_with_quote(2, Calendar.israel("Tase"), BDC.Unadjusted, SimpleQuote(0.2), DayCounter.actual360(), VolatilityType.ShiftedLognormal, settings)
    else:
        surface = ConstantOptionletVolatility.moving(2, Calendar.israel("Tase"), BDC.Unadjusted, 0.2, DayCounter.actual360(), VolatilityType.ShiftedLognormal, settings)
    bond.set_coupon_pricer(BlackIborCouponPricer(surface))
    assert view.amount() > 0.0
    index.add_fixing(Date(13, 1, 2024), 0.0387)
    settings.set_evaluation_date(Date(1, 1, 2060))
    assert view.amount() > 0.0
    bond.set_coupon_pricer(BlackIborCouponPricer(_vol()))
    assert view.amount() > 0.0


def test_custom_index_maturity_calendar_metadata_retained_after_original_wrappers_drop():
    """Keep custom maturity-calendar horizons when the returned index is reused."""
    settings = Settings()
    settings.set_evaluation_date(Date(5, 1, 2024))
    calendar = Calendar.israel("Tase")
    curve = FlatForward(Date(5, 1, 2024), 0.03, DayCounter.actual360())
    index = CustomIborIndex("custom-bound", Period(3, "Months"), 2, Currency.eur(), Calendar.null_calendar(), Calendar.null_calendar(), calendar, BDC.Unadjusted, False, DayCounter.actual360(), curve, settings)
    bond = _bond(settings, index)
    retained = bond.index()
    del index, calendar, curve, bond
    gc.collect()
    deferred = _bond(settings, retained, start=Date(15, 1, 2060), end=Date(15, 1, 2061))
    with pytest.raises(ItofinError):
        deferred.cashflows()[0].amount()


def test_irrelevant_moving_surface_does_not_block_plain_known_or_redemption_views():
    """Only forecast optionlets validate the attached moving surface reference date."""
    settings = Settings()
    settings.set_evaluation_date(Date(5, 1, 2024))
    index, discount, _ = _market(settings)
    bond = _bond(settings, index, caps=[None, 0.06])
    bond.set_engine(DiscountingBondEngine(discount, settings))
    leg = bond.cashflows()
    plain, capped, redemption = leg[0], leg[1], leg[-1]
    bond.set_coupon_pricer(BlackIborCouponPricer(_vol(settings, settlement_days=100_000)))
    assert plain.amount() > 0.0
    assert redemption.amount() == 1000.0
    assert bond.redemptions()[0].amount() == 1000.0
    with pytest.raises(ItofinError):
        capped.amount()
    with pytest.raises(ItofinError):
        bond.npv()
    index.add_fixing(Date(13, 1, 2024), 0.0387)
    index.add_fixing(Date(13, 4, 2024), 0.0387)
    settings.set_evaluation_date(Date(16, 4, 2024))
    assert plain.amount() > 0.0
    assert capped.amount() > 0.0
    all_plain = _bond(settings, index, caps=[None])
    all_plain.set_coupon_pricer(BlackIborCouponPricer(_vol(settings, settlement_days=100_000)))
    all_plain.set_engine(DiscountingBondEngine(discount, settings))
    assert all_plain.npv() > 0.0


def test_expired_leg_ex_coupon_and_unused_scalar_paths_ignore_bad_volatility():
    """Preflight precisely the actual amount set, respecting expiry and ex-coupon filtering."""
    settings = Settings()
    settings.set_evaluation_date(Date(5, 1, 2024))
    index, discount, _ = _market(settings)
    bond = _bond(settings, index, caps=[0.04], ex_coupon_period=Period(10, "Years"), ex_coupon_calendar=Calendar.null_calendar())
    bond.set_engine(DiscountingBondEngine(discount, settings))
    bond.set_coupon_pricer(BlackIborCouponPricer(_vol(settings, settlement_days=100_000)))
    leg = bond.cashflows()
    assert bond.npv() > 0.0
    assert bond.clean_price() > 0.0
    assert bond.accrued_amount(Date(13, 1, 2024)) == 0.0
    assert bond.notional(Date(1, 1, 2030)) == 0.0
    assert npv(leg, discount, settings, settlement_date=Date(1, 1, 2030)) == 0.0
    with pytest.raises(ItofinError):
        leg[0].amount()
    settings.set_evaluation_date(Date(1, 1, 2030))
    assert bond.npv() == 0.0
    assert bond.settlement_value() == 0.0


def test_frozen_currency_cache_skips_unqueried_replacement_surface():
    """Replacing a pricer while frozen does not query its unused volatility surface."""
    settings = Settings()
    settings.set_evaluation_date(Date(5, 1, 2024))
    index, discount, _ = _market(settings)
    bond = _bond(settings, index, caps=[0.04])
    bond.set_engine(DiscountingBondEngine(discount, settings))
    bond.set_coupon_pricer(BlackIborCouponPricer(_vol()))
    initial = bond.npv()
    bond.freeze()
    bond.set_coupon_pricer(BlackIborCouponPricer(_vol(settings, settlement_days=100_000)))
    assert bond.npv() == initial
    assert bond.redemptions()[0].amount() == 1000.0
    with pytest.raises(ItofinError):
        bond.recalculate()
    bond.unfreeze()
    with pytest.raises(ItofinError):
        bond.npv()


def test_israel_factory_records_existing_holiday_horizon_without_changing_valid_dates():
    """Report an out-of-table calendar date as a normal error rather than a panic."""
    calendar = Calendar.israel("Tase")
    assert isinstance(calendar.is_business_day(Date(15, 1, 2024)), bool)
    with pytest.raises(ItofinError):
        calendar.is_business_day(Date(1, 1, 2060))


def test_todays_unfixed_cap_uses_index_forecast_but_intrinsic_optionlet_without_surface():
    """A fixing forecast today needs geometry but not a future optionlet surface."""
    settings = Settings()
    settings.set_evaluation_date(Date(13, 1, 2024))
    index, _, _ = _market(settings)
    bond = _bond(settings, index, caps=[0.04])
    bond.set_coupon_pricer(BlackIborCouponPricer(_vol(settings, settlement_days=100_000)))
    assert bond.cashflows()[0].amount() > 0.0


def test_frozen_quotes_keep_currency_results_but_guard_current_accrual_dependencies():
    """Frozen dirty currency pricing and live clean/accrual queries have distinct guards."""
    settings = Settings()
    settings.set_evaluation_date(Date(19, 6, 2026))
    calendar = Calendar.weekends_only()
    index, discount, _ = _market(settings, calendar)
    schedule = Schedule(Date(24, 6, 2026), Date(24, 6, 2027), Frequency.Quarterly, calendar, BDC.Unadjusted)
    bond = FloatingRateBond(5, 1000.0, schedule, index, DayCounter.actual360(), settings, caps=[0.04])
    bond.set_engine(DiscountingBondEngine(discount, settings))
    bond.set_coupon_pricer(BlackIborCouponPricer(_vol()))
    initial = [bond.npv(), bond.settlement_value(), bond.dirty_price()]
    assert bond.accrued_amount() > 0.0
    assert bond.settlement_date() == Date(26, 6, 2026)
    bond.freeze()
    bond.set_coupon_pricer(BlackIborCouponPricer(_vol(settings, settlement_days=100_000)))
    assert [bond.npv(), bond.settlement_value(), bond.dirty_price()] == initial
    assert bond.accrued_amount(Date(19, 6, 2026)) == 0.0
    with pytest.raises(ItofinError):
        bond.clean_price()
    with pytest.raises(ItofinError):
        bond.accrued_amount()


def test_mixed_calendar_natural_roll_does_not_invent_fixing_calendar_adjustment():
    """Use the actual custom value roll when the cached value lies beyond a holiday table."""
    settings = Settings()
    settings.set_evaluation_date(Date(24, 12, 2014))
    forwarding = FlatForward(Date(24, 12, 2014), 0.035, DayCounter.actual360())
    index = CustomIborIndex("mixed-bound", Period(3, "Months"), 3, Currency.eur(), Calendar.japan(), Calendar.null_calendar(), Calendar.indonesia(), BDC.Unadjusted, False, DayCounter.actual360(), forwarding, settings)
    schedule = Schedule(Date(29, 12, 2014), Date(29, 3, 2015), Frequency.Quarterly, Calendar.null_calendar(), BDC.Unadjusted)
    bond = FloatingRateBond(2, 1000.0, schedule, index, DayCounter.actual360(), settings, fixing_days=1)
    assert bond.cashflows()[0].amount() == pytest.approx(8.786682023998239, abs=2e-12)


@pytest.mark.parametrize("factory,market,horizon", [
    ("china", "SSE", 2026), ("china", "IB", 2026),
    ("hong_kong", "HKEX", 2025), ("india", "NSE", 2026),
    ("indonesia", "BEJ", 2014), ("israel", "Tase", 2050),
    ("israel", "Telbor", 2050), ("israel", "Shir", 2050),
    ("new_zealand", "Wellington", 2052), ("new_zealand", "Auckland", 2052),
    ("north_macedonia", "MSE", 2040), ("saudi_arabia", "Tadawul", 2022),
    ("singapore", "SGX", 2026), ("south_korea", "Settlement", 2050),
    ("south_korea", "KRX", 2050), ("taiwan", "TSEC", 2026),
    ("thailand", None, 2025), ("turkey", None, 2034),
    ("uzbekistan", "UZSE", 2040),
])
def test_known_calendar_tables_and_returned_index_getters_keep_typed_bounds(factory, market, horizon):
    """Known native tables retain valid behavior and typed limits through returned indices."""
    constructor = getattr(Calendar, factory)
    calendar = constructor() if market is None else constructor(market)
    valid, unsupported = Date(5, 1, horizon), Date(5, 1, horizon + 1)
    expected = calendar.is_business_day(valid)
    settings = Settings()
    settings.set_evaluation_date(Date(5, 1, 2014))
    index = IborIndex("returned-calendar-bound", Period(3, "Months"), 2, Currency.eur(), calendar, BDC.Unadjusted, False, DayCounter.actual360(), None, settings)
    returned = index.fixing_calendar()
    assert returned.name == calendar.name
    assert returned.is_business_day(valid) == expected
    for view in [calendar, returned]:
        with pytest.raises(ItofinError):
            view.is_business_day(unsupported)


def test_floating_returned_index_keeps_calendar_bounds_after_parent_release():
    """The new floating index inspector retains actual calendar metadata after owner GC."""
    settings = Settings()
    settings.set_evaluation_date(Date(5, 1, 2014))
    calendar = Calendar.indonesia()
    index, _, _ = _market(settings, calendar)
    bond = _bond(settings, index, start=Date(15, 1, 2014), end=Date(15, 10, 2014))
    returned = bond.index().fixing_calendar()
    expected = calendar.is_business_day(Date(5, 1, 2014))
    del bond, index, calendar
    gc.collect()
    assert returned.is_business_day(Date(5, 1, 2014)) == expected
    with pytest.raises(ItofinError):
        returned.is_business_day(Date(5, 1, 2015))


@pytest.mark.parametrize("rule", ["JoinHolidays", "JoinBusinessDays"])
def test_joint_calendar_retains_child_horizon_and_first_year(rule):
    """Joint views preserve all known native child table bounds without changing valid dates."""
    calendar = Calendar.joint([Calendar.indonesia(), Calendar.russia("MOEX")], rule)
    assert isinstance(calendar.is_business_day(Date(5, 1, 2014)), bool)
    for unsupported in [Date(5, 1, 2011), Date(5, 1, 2015)]:
        with pytest.raises(ItofinError):
            calendar.is_business_day(unsupported)


def test_moex_first_year_is_retained_and_settlement_market_is_unrestricted():
    """Only the MOEX market uses the native first-year restriction."""
    moex = Calendar.russia("MOEX")
    assert isinstance(moex.is_business_day(Date(5, 1, 2012)), bool)
    with pytest.raises(ItofinError):
        moex.is_business_day(Date(5, 1, 2011))
    assert isinstance(Calendar.russia("Settlement").is_business_day(Date(5, 1, 2011)), bool)


def test_unadjusted_arithmetic_beyond_irrelevant_holiday_table_matches_native():
    """Pure month/zero-step arithmetic need not query an unrelated maturity holiday table."""
    expected = json.loads((Path(__file__).parent / "fixtures/floating_bindings_native.json").read_text())["calendar_arithmetic"]
    settings = Settings()
    today = Date(24, 12, 2014)
    settings.set_evaluation_date(today)
    settings.set_using_at_par_coupons(True)
    dc = DayCounter.actual360()
    forwarding = FlatForward(today, 0.043, dc)
    discount = FlatForward(today, 0.023, dc)
    index = CustomIborIndex("UnadjustedCalendarArithmetic", Period(3, "Months"), 3, Currency.eur(), Calendar.japan(), Calendar.null_calendar(), Calendar.indonesia(), BDC.Unadjusted, False, dc, forwarding, settings)
    schedule = Schedule(Date(30, 12, 2014), Date(30, 3, 2015), Frequency.Quarterly, Calendar.null_calendar(), BDC.Unadjusted)
    bond = FloatingRateBond(0, 375.0, schedule, index, dc, settings, payment_convention=BDC.Unadjusted, fixing_days=2, gearings=[1.25], spreads=[0.002], redemption=101.0)
    bond.set_engine(DiscountingBondEngine(discount, settings))
    assert bond.cashflows()[0].amount() == pytest.approx(expected["amount"], abs=3e-12, rel=3e-13)
    assert bond.npv() == pytest.approx(expected["npv"], abs=3e-11, rel=3e-13)
    assert bond.dirty_price() == pytest.approx(expected["dirty"], abs=3e-11, rel=3e-13)


@pytest.mark.parametrize("limited_value", [False, True])
@pytest.mark.parametrize("fixing_before_construction", [False, True])
def test_custom_calendar_forecast_guards_are_deferred_and_history_recovers(limited_value, fixing_before_construction):
    """Historical coupons do not query future value/maturity calendars during construction."""
    settings = Settings()
    today = Date(28, 9, 2026)
    settings.set_evaluation_date(today)
    unlimited, limited, dc = Calendar.null_calendar(), Calendar.china(), DayCounter.actual360()
    value_calendar, maturity_calendar = (limited, unlimited) if limited_value else (unlimited, limited)
    start, end, fixing = (Date(4, 1, 2027), Date(4, 7, 2027), Date(2, 1, 2027)) if limited_value else (Date(5, 10, 2026), Date(5, 4, 2027), Date(3, 10, 2026))
    curve = FlatForward(today, 0.04, dc)
    index = CustomIborIndex("deferred-custom-bound", Period(6, "Months"), 2, Currency.eur(), unlimited, value_calendar, maturity_calendar, BDC.Following, False, dc, curve, settings)
    if fixing_before_construction:
        settings.set_evaluation_date(start)
        index.add_fixing(fixing, 0.037)
    schedule = Schedule(start, end, Frequency.Semiannual, unlimited, BDC.Unadjusted)
    bond = FloatingRateBond(0, 250.0, schedule, index, dc, settings, caps=[0.04])
    flow = bond.cashflows()[0]
    bond.set_coupon_pricer(BlackIborCouponPricer())
    del limited, value_calendar, maturity_calendar, unlimited, schedule
    gc.collect()
    if not fixing_before_construction:
        with pytest.raises(ItofinError, match="2026"):
            flow.amount()
        settings.set_evaluation_date(start)
        index.add_fixing(fixing, 0.037)
    assert flow.amount() == pytest.approx(250.0 * 0.037 * dc.year_fraction(start, end), abs=3e-12, rel=3e-13)
    native = json.loads((Path(__file__).parent / "fixtures/floating_bindings_native.json").read_text())["deferred_constructor_cases"]
    assert flow.amount() == pytest.approx(next(row["historical_amount"] for row in native if row["limited_value"] == limited_value), abs=3e-12, rel=3e-13)


def test_constructor_needs_no_evaluation_date_history_or_pricer():
    """Deferred forecast metadata does not make bond construction demand market data."""
    settings = Settings()
    calendar, dc = Calendar.null_calendar(), DayCounter.actual360()
    index = CustomIborIndex("deferred-no-market-data", Period(6, "Months"), 2, Currency.eur(), calendar, Calendar.china(), Calendar.china(), BDC.Following, False, dc, None, settings)
    schedule = Schedule(Date(4, 1, 2027), Date(4, 7, 2027), Frequency.Semiannual, calendar, BDC.Unadjusted)
    bond = FloatingRateBond(0, 250.0, schedule, index, dc, settings, caps=[0.04])
    assert len(bond.cashflows()) == 2
    assert bond.cashflows()[0].date() == Date(4, 7, 2027)


def test_indexed_policy_skips_unused_at_par_end_roll_and_frozen_cache_recovers():
    """Indexed forecasts use no next-fixing roll; cached frozen NPV uses no new forecast."""
    native = json.loads((Path(__file__).parent / "fixtures/floating_bindings_native.json").read_text())["indexed_horizon_case"]
    settings = Settings()
    today = Date(28, 9, 2026)
    settings.set_evaluation_date(today)
    settings.set_using_at_par_coupons(False)
    calendar, dc = Calendar.null_calendar(), DayCounter.actual360()
    forwarding = FlatForward(today, 0.04, dc)
    index = CustomIborIndex("indexed-mode-bound", Period(6, "Months"), 2, Currency.eur(), Calendar.china(), calendar, calendar, BDC.Following, False, dc, forwarding, settings)
    schedule = Schedule(Date(5, 10, 2026), Date(5, 4, 2027), Frequency.Semiannual, calendar, BDC.Unadjusted)
    bond = FloatingRateBond(0, 250.0, schedule, index, dc, settings)
    bond.set_engine(DiscountingBondEngine(forwarding, settings))
    flow = bond.cashflows()[0]
    assert flow.amount() == pytest.approx(native["amount"], abs=3e-12, rel=3e-13)
    initial = bond.npv()
    bond.freeze()
    settings.set_using_at_par_coupons(True)
    assert bond.npv() == initial
    with pytest.raises(ItofinError, match="2026"):
        flow.amount()
    bond.unfreeze()
    with pytest.raises(ItofinError, match="2026"):
        bond.npv()
    settings.set_using_at_par_coupons(False)
    assert bond.npv() == initial
    assert flow.amount() == pytest.approx(native["amount"], abs=3e-12, rel=3e-13)


def test_unadjusted_zero_lag_still_checks_runtime_index_fixing_validity():
    """Skipped arithmetic holiday lookups do not skip the index's actual fixing query."""
    settings = Settings()
    today = Date(28, 9, 2026)
    settings.set_evaluation_date(today)
    calendar, dc = Calendar.null_calendar(), DayCounter.actual360()
    curve = FlatForward(today, 0.04, dc)
    index = CustomIborIndex("fixing-query-bound", Period(6, "Months"), 0, Currency.eur(), Calendar.china(), calendar, calendar, BDC.Unadjusted, False, dc, curve, settings)
    schedule = Schedule(Date(4, 1, 2027), Date(4, 7, 2027), Frequency.Semiannual, calendar, BDC.Unadjusted)
    bond = FloatingRateBond(0, 250.0, schedule, index, dc, settings, fixing_days=0, fixing_convention=BDC.Unadjusted)
    with pytest.raises(ItofinError, match="2026"):
        bond.cashflows()[0].amount()
