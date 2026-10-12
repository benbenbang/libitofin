"""Pure inherited bond/leg analytics pinned to independent QuantLib 1.43 oracles."""

import gc
import json
import math
from pathlib import Path
from typing import Any, cast

import pytest

from itofin import ItofinError, Settings
from itofin.cashflows import BlackIborCouponPricer, npv_at_z_spread, z_spread
from itofin.indexes import Currency, IborIndex
from itofin.instruments import FixedRateBond, FloatingRateBond, ZeroCouponBond
from itofin.pricingengines import DiscountingBondEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import BondPriceType, ConstantOptionletVolatility, FlatForward, VolatilityType
from itofin.time import BusinessDayConvention as BDC, Calendar, Compounding, Date, DateGeneration, DayCounter, Frequency, Period, Schedule

FIXTURES = Path(__file__).parent / "fixtures"
NATIVE = json.loads((FIXTURES / "bond_analytics_native.json").read_text())
INCLUSION = json.loads((FIXTURES / "bond_analytics_inclusion_native.json").read_text())
FROZEN = json.loads((FIXTURES / "bond_analytics_frozen_native.json").read_text())


def close(value, expected):
    """Compare against the predeclared independent price tolerance."""
    assert value == pytest.approx(expected, abs=3e-12, rel=3e-13)


def build(row, settings=None, lag=None):
    """Build an unpriced fixed/zero native fixture on explicit Settings."""
    settings = settings or Settings()
    dc, calendar = DayCounter.actual360(), Calendar.null_calendar()
    days = row["settlement_days"] if lag is None else lag
    if row["kind"] == "zero":
        bond = ZeroCouponBond(days, calendar, row["face"], Date(*row["end"]), settings, payment_convention=BDC.Unadjusted, redemption=row["redemption"])
    else:
        schedule = Schedule(Date(*row["start"]), Date(*row["end"]), Frequency.Semiannual, calendar, BDC.Unadjusted, DateGeneration.Backward)
        options = {"ex_coupon_period": Period(row["ex_days"], "Days"), "ex_coupon_calendar": calendar} if row["ex_days"] else {}
        bond = FixedRateBond(days, row["face"], schedule, row["coupons"], dc, settings, payment_convention=BDC.Unadjusted, redemption=row["redemption"], **options)
    return bond, settings, dc, FlatForward(Date(*row["reference"]), row["curve_rate"], dc)


@pytest.mark.parametrize("row", NATIVE["cases"], ids=[row["name"] for row in NATIVE["cases"]])
def test_all_ten_routes_native_without_engine_or_evaluation_date(row):
    """Explicit settlement is unchanged even with lag, and every route is engine independent."""
    bond, settings, dc, curve = build(row)
    expected, settlement = row["expected"], Date(*row["settlement"])
    mode, frequency = getattr(Compounding, row["compounding"]), getattr(Frequency, row["frequency"])
    args = (dc, mode, frequency)
    close(bond.clean_price_from_curve(curve, settlement=settlement), expected["clean_curve"])
    close(bond.dirty_price_from_curve(curve, settlement=settlement), expected["dirty_curve"])
    close(bond.clean_price_from_yield(row["yield_rate"], *args, settlement=settlement), expected["clean_yield"])
    close(bond.dirty_price_from_yield(row["yield_rate"], *args, settlement=settlement), expected["dirty_yield"])
    close(bond.clean_price_at_z_spread(curve, row["z_spread"], mode, frequency, settlement=settlement), expected["clean_z_spread"])
    close(bond.dirty_price_at_z_spread(curve, row["z_spread"], mode, frequency, settlement=settlement), expected["dirty_z_spread"])
    for price_type, suffix in [(BondPriceType.Clean, "clean"), (BondPriceType.Dirty, "dirty")]:
        close(bond.yield_rate(expected[suffix+"_yield"], *args, price_type=price_type, settlement=settlement, accuracy=1e-12, max_evaluations=1000), expected["yield_from_"+suffix])
        close(bond.z_spread(expected[suffix+"_z_spread"], curve, mode, frequency, price_type=price_type, settlement=settlement, accuracy=1e-12, max_evaluations=1000), expected["z_spread_from_"+suffix])
    leg = bond.cashflows()
    options = {"settlement_date": settlement, "npv_date": Date(*row["reference"]), "include_settlement_date_flows": False}
    close(npv_at_z_spread(leg, curve, row["z_spread"], mode, frequency, settings, **options), expected["leg_npv_distinct_npv_date"])
    close(z_spread(leg, expected["leg_npv_distinct_npv_date"], curve, mode, frequency, settings, accuracy=1e-12, max_evaluations=1000, **options), expected["leg_z_spread"])
    close(bond.accrued_amount(settlement), expected["accrued"])
    close(bond.notional(settlement), expected["notional"])
    assert not bond.is_calculated()
    with pytest.raises(ItofinError):
        bond.dirty_price_from_curve(curve)


@pytest.mark.parametrize("row", INCLUSION["rows"])
def test_leg_tri_state_inclusion_and_distinct_npv_date_native(row):
    """Preserve tri-state Settings inclusion and currency NPV date normalization."""
    c = INCLUSION
    settings = Settings()
    settings.set_evaluation_date(Date(*c["settlement"]))
    settings.set_include_reference_date_events(row["include_reference"])
    settings.set_include_todays_cash_flows(row["include_today"])
    dc, calendar = DayCounter.actual360(), Calendar.null_calendar()
    schedule = Schedule(Date(*c["start"]), Date(*c["end"]), Frequency.Semiannual, calendar, BDC.Unadjusted, DateGeneration.Backward)
    bond = FixedRateBond(0, c["face"], schedule, [c["coupon"]], dc, settings, redemption=c["redemption"])
    curve = FlatForward(Date(*c["reference"]), c["curve_rate"], dc)
    args = (bond.cashflows(), curve, Compounding.Compounded, Frequency.Quarterly, settings)
    options = {"settlement_date": Date(*c["settlement"]), "npv_date": Date(*c["npv_date"]), "include_settlement_date_flows": row["include_settlement_flows"]}
    close(npv_at_z_spread(args[0], args[1], c["z_spread"], *args[2:], **options), row["npv"])
    close(z_spread(args[0], row["npv"], *args[1:], accuracy=1e-12, max_evaluations=1000, **options), row["z_spread"])


def floating_market():
    """Build retained live forward/volatility inputs for frozen analytical queries."""
    c = FROZEN
    settings = Settings()
    settings.set_evaluation_date(Date(*c["eval"]))
    dc, calendar = DayCounter.actual360(), Calendar.target()
    forward_quote, vol_quote = SimpleQuote(c["initial_forward"]), SimpleQuote(c["initial_volatility"])
    forwarding = FlatForward.from_quote(Date(*c["eval"]), forward_quote, dc)
    discount = FlatForward(Date(*c["eval"]), c["engine_curve_rate"], dc)
    analytics = FlatForward(Date(*c["reference"]), c["analytics_curve_rate"], dc)
    index = IborIndex("analytics-floating", Period(3, "Months"), 2, Currency.eur(), calendar, BDC.ModifiedFollowing, False, dc, forwarding, settings)
    schedule = Schedule(Date(*c["start"]), Date(*c["end"]), Frequency.Quarterly, calendar, BDC.Unadjusted, DateGeneration.Backward)
    bond = FloatingRateBond(c["settlement_days"], c["face"], schedule, index, dc, settings, gearings=[c["gearing"]], spreads=[c["spread"]], caps=[c["cap"]], redemption=c["redemption"])
    vol = ConstantOptionletVolatility.with_quote(Date(*c["eval"]), calendar, BDC.Following, vol_quote, dc, VolatilityType.ShiftedLognormal)
    bond.set_coupon_pricer(BlackIborCouponPricer(vol))
    bond.set_engine(DiscountingBondEngine(discount, settings))
    return bond, settings, dc, analytics, forward_quote, vol_quote


def check_floating_snapshot(bond, settings, dc, curve, expected):
    """Pin all ten routes to current native coupon market data."""
    settlement = Date(*FROZEN["settlement"])
    mode, frequency = Compounding.Compounded, Frequency.Semiannual
    close(bond.clean_price_from_curve(curve, settlement=settlement), expected["clean_curve"])
    close(bond.dirty_price_from_curve(curve, settlement=settlement), expected["dirty_curve"])
    close(bond.clean_price_from_yield(0.052, dc, mode, frequency, settlement=settlement), expected["clean_yield"])
    close(bond.dirty_price_from_yield(0.052, dc, mode, frequency, settlement=settlement), expected["dirty_yield"])
    close(bond.yield_rate(expected["clean_curve"], dc, mode, frequency, settlement=settlement, accuracy=1e-12, max_evaluations=1000), expected["yield_rate"])
    close(bond.clean_price_at_z_spread(curve, 0.007, mode, frequency, settlement=settlement), expected["clean_z_spread"])
    close(bond.dirty_price_at_z_spread(curve, 0.007, mode, frequency, settlement=settlement), expected["dirty_z_spread"])
    close(bond.z_spread(expected["clean_z_spread"], curve, mode, frequency, settlement=settlement, accuracy=1e-12, max_evaluations=1000), expected["z_spread"])
    leg = bond.cashflows()
    options = {"settlement_date": settlement, "npv_date": Date(*FROZEN["reference"]), "include_settlement_date_flows": False}
    close(npv_at_z_spread(leg, curve, 0.007, mode, frequency, settings, **options), expected["leg_npv"])
    close(z_spread(leg, expected["leg_npv"], curve, mode, frequency, settings, accuracy=1e-12, max_evaluations=1000, **options), expected["leg_z_spread"])


def test_frozen_analytics_are_current_native_and_do_not_change_cached_engine_state():
    """Current analytical market data leaves the frozen engine cache untouched."""
    bond, settings, dc, curve, forward, vol = floating_market()
    close(bond.npv(), FROZEN["cached_engine_npv"])
    bond.freeze()
    assert bond.is_calculated()
    check_floating_snapshot(bond, settings, dc, curve, FROZEN["initial_frozen_analytics"])
    forward.set_value(FROZEN["updated_forward"])
    vol.set_value(FROZEN["updated_volatility"])
    settings.set_include_reference_date_events(True)
    before_analytics = bond.is_calculated()
    check_floating_snapshot(bond, settings, dc, curve, FROZEN["current_frozen_analytics"])
    assert bond.is_calculated() == before_analytics
    assert bond.npv() == FROZEN["cached_engine_npv"]
    bond.unfreeze()
    close(bond.npv(), FROZEN["unfrozen_engine_npv"])


def test_explicit_settlement_skips_lag_and_omitted_settlement_uses_checked_existing_path():
    """Explicit actual settlement bypasses lag and omitted dates reuse checked settlement."""
    row = NATIVE["cases"][0]
    bond, settings, _, curve = build(row, lag=100000)
    close(bond.clean_price_from_curve(curve, settlement=Date(*row["settlement"])), row["expected"]["clean_curve"])
    settings.set_evaluation_date(Date(*row["eval"]))
    with pytest.raises(ItofinError):
        bond.clean_price_from_curve(curve)
    normal, normal_settings, _, _ = build(row)
    normal_settings.set_evaluation_date(Date(*row["eval"]))
    assert normal.settlement_date() == Date(24, 6, 2026)
    close(normal.clean_price_from_curve(curve), normal.clean_price_from_curve(curve, settlement=normal.settlement_date()))


@pytest.mark.parametrize("kind", ["fixed", "zero"])
def test_default_solver_options_zero_guess_and_failed_solve_recover(kind):
    """Defaults and explicit zero guesses work; failed solves do not poison the bond."""
    row = dict(NATIVE["cases"][0], kind=kind)
    bond, _, dc, curve = build(row)
    settlement, mode, frequency = Date(*row["settlement"]), Compounding.Compounded, Frequency.Semiannual
    clean = bond.clean_price_from_yield(0.052, dc, mode, frequency, settlement=settlement)
    assert bond.yield_rate(clean, dc, mode, frequency, settlement=settlement) == pytest.approx(0.052, abs=2e-10)
    with pytest.raises(ItofinError, match="bracket"):
        bond.yield_rate(clean, dc, mode, frequency, settlement=settlement, guess=0.0)
    if kind == "zero":
        zero_quote = bond.clean_price_from_yield(0.0, dc, mode, frequency, settlement=settlement)
        assert bond.yield_rate(zero_quote, dc, mode, frequency, settlement=settlement, guess=0.0) == 0.0
    quote = bond.clean_price_at_z_spread(curve, 0.007, mode, frequency, settlement=settlement)
    assert bond.z_spread(quote, curve, mode, frequency, settlement=settlement) == pytest.approx(0.007, abs=2e-10)
    for bad in [{"accuracy": 0.0}, {"accuracy": -1.0}, {"accuracy": math.nan}, {"max_evaluations": 0}, {"max_evaluations": 1}, {"guess": math.inf}]:
        with pytest.raises(ItofinError):
            bond.yield_rate(clean, dc, mode, frequency, settlement=settlement, **bad)
        with pytest.raises(ItofinError):
            bond.z_spread(quote, curve, mode, frequency, settlement=settlement, **bad)
    close(bond.z_spread(quote, curve, mode, frequency, settlement=settlement, accuracy=1e-12, max_evaluations=1000), 0.007)


def test_enum_ordinals_invalid_types_negative_domains_and_matured_errors():
    """Validate conventions, finite inputs and actual rate domains without breaking enums."""
    for ordinal, name in enumerate(["Simple", "Compounded", "Continuous", "SimpleThenCompounded", "CompoundedThenSimple"]):
        assert int(getattr(Compounding, name)) == ordinal
    for ordinal, name in enumerate(["Annual", "Semiannual", "Quarterly", "Monthly"]):
        assert int(getattr(Frequency, name)) == ordinal
    with pytest.raises(TypeError):
        cast(Any, Compounding)(0)
    row = dict(NATIVE["cases"][0], kind="zero")
    bond, _, dc, curve = build(row)
    settlement = Date(*row["settlement"])
    for invalid in [0, -1, 99, "Compounded", None]:
        with pytest.raises(TypeError):
            bond.dirty_price_from_yield(0.05, dc, invalid, Frequency.Annual, settlement=settlement)
        with pytest.raises(TypeError):
            bond.dirty_price_from_yield(0.05, dc, Compounding.Simple, invalid, settlement=settlement)
    for mode, value in [(Compounding.Simple, -1.0), (Compounding.Compounded, -2.0)]:
        with pytest.raises(ItofinError):
            bond.dirty_price_from_yield(value, dc, mode, Frequency.Semiannual, settlement=settlement)
    for value in [math.nan, math.inf, -math.inf]:
        with pytest.raises(ItofinError):
            bond.clean_price_from_yield(value, dc, Compounding.Simple, Frequency.Annual, settlement=settlement)
        with pytest.raises(ItofinError):
            bond.dirty_price_at_z_spread(curve, value, Compounding.Simple, Frequency.Annual, settlement=settlement)
        with pytest.raises(ItofinError):
            z_spread(bond.cashflows(), value, curve, Compounding.Simple, Frequency.Annual, Settings(), settlement_date=settlement)
    with pytest.raises(ItofinError):
        bond.clean_price_from_curve(curve, settlement=Date(*row["end"]))
    with pytest.raises(TypeError):
        cast(Any, bond.clean_price_from_curve)(curve, settlement)


def test_guarded_live_analytics_fail_while_frozen_and_recover_after_pricer_replacement():
    """Required live analytics fail independently of a frozen engine cache."""
    bond, settings, dc, curve, _, _ = floating_market()
    initial = bond.npv()
    bond.freeze()
    leg = bond.cashflows()
    wrong = ConstantOptionletVolatility.moving(100000, Calendar.null_calendar(), BDC.Unadjusted, 0.2, dc, VolatilityType.ShiftedLognormal, settings)
    bond.set_coupon_pricer(BlackIborCouponPricer(wrong))
    settlement = Date(*FROZEN["settlement"])
    with pytest.raises(ItofinError):
        bond.dirty_price_from_curve(curve, settlement=settlement)
    with pytest.raises(ItofinError):
        npv_at_z_spread(leg, curve, 0.007, Compounding.Compounded, Frequency.Semiannual, settings, settlement_date=settlement)
    assert bond.npv() == initial
    fixed = ConstantOptionletVolatility(Date(*FROZEN["eval"]), Calendar.target(), BDC.Unadjusted, 0.2, dc, VolatilityType.ShiftedLognormal)
    bond.set_coupon_pricer(BlackIborCouponPricer(fixed))
    assert bond.dirty_price_from_curve(curve, settlement=settlement) > 0.0
    del fixed, wrong, bond
    gc.collect()
    assert npv_at_z_spread(leg, curve, 0.007, Compounding.Compounded, Frequency.Semiannual, settings, settlement_date=settlement) > 0.0


def test_zero_gearing_explicit_analytics_need_no_index_evaluation_curve_history_or_pricer():
    """All fixed coupon paths avoid irrelevant index market-data requirements."""
    settings = Settings()
    dc, calendar = DayCounter.actual360(), Calendar.null_calendar()
    index = IborIndex("fixed-analytics", Period(3, "Months"), 2, Currency.eur(), calendar, BDC.Unadjusted, False, dc, None, settings)
    schedule = Schedule(Date(1, 1, 2026), Date(1, 1, 2027), Frequency.Quarterly, calendar, BDC.Unadjusted)
    bond = FloatingRateBond(0, 250.0, schedule, index, dc, settings, gearings=[0.0], spreads=[0.03])
    curve = FlatForward(Date(1, 1, 2026), 0.025, dc)
    actual = Date(10, 1, 2026)
    assert bond.clean_price_from_curve(curve, settlement=actual) > 0.0
    assert bond.dirty_price_from_yield(0.03, dc, Compounding.Compounded, Frequency.Quarterly, settlement=actual) > 0.0
    assert not bond.is_calculated()


def test_dirty_routes_skip_ex_coupon_forecast_but_clean_routes_guard_negative_accrual():
    """Surviving flows and negative accrued coupons select distinct actual dependencies."""
    settings = Settings()
    settings.set_evaluation_date(Date(5, 1, 2026))
    dc, calendar = DayCounter.actual360(), Calendar.null_calendar()
    curve = FlatForward(Date(5, 1, 2026), 0.025, dc)
    index = IborIndex("ex-coupon-analytics", Period(3, "Months"), 2, Currency.eur(), calendar, BDC.Unadjusted, False, dc, curve, settings)
    schedule = Schedule(Date(15, 1, 2026), Date(15, 1, 2027), Frequency.Quarterly, calendar, BDC.Unadjusted)
    bond = FloatingRateBond(0, 250.0, schedule, index, dc, settings, caps=[0.04], ex_coupon_period=Period(10, "Years"), ex_coupon_calendar=calendar)
    bad = ConstantOptionletVolatility.moving(100000, calendar, BDC.Unadjusted, 0.2, dc, VolatilityType.ShiftedLognormal, settings)
    bond.set_coupon_pricer(BlackIborCouponPricer(bad))
    settlement = Date(1, 2, 2026)
    mode, frequency = Compounding.Compounded, Frequency.Quarterly
    dirty = bond.dirty_price_from_curve(curve, settlement=settlement)
    assert dirty > 0.0
    quote = bond.dirty_price_from_yield(0.03, dc, mode, frequency, settlement=settlement)
    assert bond.yield_rate(quote, dc, mode, frequency, price_type=BondPriceType.Dirty, settlement=settlement) == pytest.approx(0.03, abs=2e-10)
    spread_quote = bond.dirty_price_at_z_spread(curve, 0.007, mode, frequency, settlement=settlement)
    assert bond.z_spread(spread_quote, curve, mode, frequency, price_type=BondPriceType.Dirty, settlement=settlement) == pytest.approx(0.007, abs=2e-10)
    for method, args in [(bond.clean_price_from_curve, (curve,)), (bond.clean_price_from_yield, (0.03, dc, mode, frequency)), (bond.clean_price_at_z_spread, (curve, 0.007, mode, frequency)), (bond.yield_rate, (quote, dc, mode, frequency)), (bond.z_spread, (spread_quote, curve, mode, frequency))]:
        with pytest.raises(ItofinError):
            method(*args, settlement=settlement)
    assert npv_at_z_spread(bond.cashflows(), curve, 0.007, mode, frequency, settings, settlement_date=settlement) > 0.0
    assert npv_at_z_spread(bond.redemptions(), curve, 0.007, mode, frequency, settings, settlement_date=settlement) > 0.0


def test_live_silent_policy_analytics_preserve_uncached_flags_and_calculated_state():
    """Live policy reads do not consume or update the engine's cached flag snapshot."""
    settings = Settings()
    settings.set_evaluation_date(Date(5, 1, 2024))
    dc, calendar = DayCounter.actual360(), Calendar.null_calendar()
    curve = FlatForward(Date(5, 1, 2024), 0.035, dc)
    index = IborIndex("analytics-policy", Period(3, "Months"), 2, Currency.eur(), calendar, BDC.Unadjusted, False, dc, curve, settings)
    schedule = Schedule(Date(16, 2, 2024), Date(15, 1, 2027), Frequency.Quarterly, calendar, BDC.Unadjusted, DateGeneration.Backward)
    bond = FloatingRateBond(0, 250.0, schedule, index, dc, settings)
    bond.set_engine(DiscountingBondEngine(curve, settings))
    original = bond.npv()
    settlement = Date(5, 1, 2024)
    at_par = bond.dirty_price_from_curve(curve, settlement=settlement)
    settings.set_using_at_par_coupons(False)
    assert bond.is_calculated()
    indexed = bond.dirty_price_from_curve(curve, settlement=settlement)
    assert indexed != at_par
    assert bond.is_calculated()
    settings.set_using_at_par_coupons(True)
    assert bond.npv() == original
