"""Callable construction, inherited analytics, ownership and boundary contracts."""

import gc
import math
import json
from pathlib import Path
from typing import Any, cast

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import Bond, Callability, CallabilityType, CallableFixedRateBond, CallableZeroCouponBond, FixedRateBond, ZeroCouponBond
from itofin.pricingengines import DiscountingBondEngine
from itofin.termstructures import BondPriceType, FlatForward
from itofin.time import BusinessDayConvention as BDC, Calendar, Compounding, Date, DateGeneration, DayCounter, Frequency, Period, Schedule


def settings():
    """Build an explicit shared evaluation policy."""
    result = Settings()
    result.set_evaluation_date(Date(15, 3, 2026))
    return result


def fixed(policy, events=None, calendar=None, **options):
    """Build a schedule-native callable fixed bond."""
    schedule = Schedule(Date(15, 1, 2026), Date(15, 1, 2030), Frequency.Annual, calendar or Calendar.null_calendar(), BDC.Unadjusted)
    return CallableFixedRateBond(2, 250.0, schedule, [0.045], DayCounter.actual360(), events or [], policy, **options)


def zero(policy, events=None, calendar=None, maturity=None, **options):
    """Build a callable redemption with an explicit payment day counter."""
    return CallableZeroCouponBond(2, calendar or Calendar.weekends_only(), 250.0, maturity or Date(17, 1, 2027), DayCounter.actual360(), events or [], policy, **options)


@pytest.mark.parametrize("kind", [CallabilityType.Call, CallabilityType.Put])
@pytest.mark.parametrize("quote", [BondPriceType.Clean, BondPriceType.Dirty])
def test_callability_snapshot_preserves_direction_price_and_order(kind, quote):
    """Preserve mixed, tied, unsorted, zero-priced and past exercise records."""
    assert int(CallabilityType.Call) == 0
    assert int(CallabilityType.Put) == 1
    assert int(BondPriceType.Clean) == 0
    assert int(BondPriceType.Dirty) == 1
    policy = settings()
    call = Callability(0.0, kind, Date(1, 1, 2027), quote)
    assert (call.price(), call.price_type(), call.call_type(), call.date()) == (0.0, quote, kind, Date(1, 1, 2027))
    events = [Callability(101.0, CallabilityType.Put, Date(1, 1, 2027)), call, Callability(99.0, CallabilityType.Call, Date(1, 1, 2026))]
    for factory in [fixed, zero]:
        bond = factory(policy, events)
        snapshot = bond.callability()
        events_copy = [(e.price(), e.price_type(), e.call_type(), e.date()) for e in events]
        assert [(e.price(), e.price_type(), e.call_type(), e.date()) for e in snapshot] == events_copy
        snapshot.clear()
        assert len(bond.callability()) == 3
    events.clear()
    assert len(bond.callability()) == 3


@pytest.mark.parametrize("factory", [fixed, zero])
def test_inherited_base_and_pure_analytics_are_contractual_cashflows(factory):
    """Inherited pure analytics ignore optionality and leave lazy state untouched."""
    policy = settings()
    bond = factory(policy, [Callability(1.0, CallabilityType.Call, Date(1, 1, 2027))])
    assert isinstance(bond, Bond)
    assert bond.settlement_days() == 2
    assert bond.settlement_date() == Date(17, 3, 2026)
    assert bond.notional() == 250.0
    assert len(bond.redemptions()) == 1
    assert bond.issue_date() is None
    assert not bond.is_expired()
    assert not bond.is_calculated()
    curve = FlatForward(Date(15, 3, 2026), 0.031, DayCounter.actual360())
    if factory is fixed:
        assert isinstance(bond, CallableFixedRateBond)
        schedule = Schedule(Date(15, 1, 2026), Date(15, 1, 2030), Frequency.Annual, Calendar.null_calendar(), BDC.Unadjusted)
        plain = FixedRateBond(2, 250.0, schedule, [0.045], DayCounter.actual360(), policy)
        assert bond.frequency() == Frequency.Annual
        assert bond.day_counter() == DayCounter.actual360()
    else:
        assert isinstance(bond, CallableZeroCouponBond)
        plain = ZeroCouponBond(2, Calendar.weekends_only(), 250.0, Date(17, 1, 2027), policy)
        assert bond.day_counter() == DayCounter.actual360()
    bond.freeze()
    clean = bond.clean_price_from_curve(curve)
    assert clean == plain.clean_price_from_curve(curve)
    assert bond.dirty_price_from_curve(curve) == plain.dirty_price_from_curve(curve)
    args = (0.04, DayCounter.actual360(), Compounding.Compounded, Frequency.Annual)
    price = bond.clean_price_from_yield(*args)
    assert price == plain.clean_price_from_yield(*args)
    assert bond.dirty_price_from_yield(*args) == plain.dirty_price_from_yield(*args)
    assert bond.yield_rate(price, *args[1:]) == pytest.approx(0.04, abs=1e-10)
    spread_args = (curve, 0.007, Compounding.Compounded, Frequency.Annual)
    spread_price = bond.clean_price_at_z_spread(*spread_args)
    assert spread_price == plain.clean_price_at_z_spread(*spread_args)
    assert bond.dirty_price_at_z_spread(*spread_args) == plain.dirty_price_at_z_spread(*spread_args)
    assert bond.z_spread(spread_price, curve, Compounding.Compounded, Frequency.Annual) == pytest.approx(0.007, abs=1e-10)
    assert not bond.is_calculated()
    bond.unfreeze()
    with pytest.raises(ItofinError, match="engine"):
        bond.npv()
    assert not bond.is_calculated()


@pytest.mark.parametrize("factory", [fixed, zero])
def test_wrong_discount_engine_and_missing_engine_are_atomic(factory):
    """Reject generic discount engines without populating the callable cache."""
    policy = settings()
    bond = factory(policy)
    curve = FlatForward(Date(15, 3, 2026), 0.03, DayCounter.actual360())
    for engine_policy in [policy, Settings()]:
        with pytest.raises(ItofinError, match="noncallable"):
            bond.set_engine(DiscountingBondEngine(curve, engine_policy))
    assert not bond.is_calculated()
    for action in [bond.npv, bond.settlement_value, bond.dirty_price, bond.clean_price, bond.recalculate]:
        with pytest.raises(ItofinError, match="engine"):
            action()
        assert not bond.is_calculated()
    bond.freeze()
    with pytest.raises(ItofinError):
        bond.npv()
    bond.unfreeze()
    policy.set_evaluation_date(Date(16, 1, 2030) if factory is fixed else Date(19, 1, 2027))
    bond.freeze()
    assert bond.is_expired()
    assert bond.npv() == bond.settlement_value() == bond.clean_price() == bond.dirty_price() == 0.0
    assert bond.is_calculated()
    bond.recalculate()
    assert bond.npv() == 0.0


def test_ex_coupon_calendar_ownership_and_zero_redemption():
    """Retain real legs, settings and calendar metadata beyond input lifetime."""
    policy = settings()
    bond = fixed(policy, ex_coupon_period=Period(7, "Days"), ex_coupon_calendar=Calendar.null_calendar(), ex_coupon_convention=BDC.Unadjusted, ex_coupon_end_of_month=True, redemption=0.0, issue_date=Date(1, 4, 2026))
    assert bond.settlement_date() == Date(1, 4, 2026)
    assert bond.accrued_amount(Date(12, 1, 2027)) < 0.0
    leg = bond.cashflows()
    assert leg[-1].amount() == 0.0
    single = zero(policy, redemption=0.0)
    assert single.redemptions()[0].amount() == 0.0
    policy.set_evaluation_date(Date(31, 12, 2199))
    with pytest.raises(ItofinError, match="range"):
        single.settlement_date()
    policy = settings()
    guarded_zero = zero(policy, calendar=Calendar.indonesia(), maturity=Date(15, 1, 2014))
    policy.set_evaluation_date(Date(1, 1, 2045))
    with pytest.raises(ItofinError):
        guarded_zero.settlement_date()
    del bond, single, policy, guarded_zero
    gc.collect()
    assert leg[-1].amount() == 0.0


@pytest.mark.parametrize("value", [-1.0, math.inf, -math.inf, math.nan])
def test_invalid_callability_and_redemption(value):
    """Reject invalid prices while keeping zero as a supported exercise amount."""
    with pytest.raises(ItofinError):
        Callability(value, CallabilityType.Call, Date(1, 1, 2027))
    for factory in [fixed, zero]:
        with pytest.raises(ItofinError):
            factory(settings(), redemption=value)


def test_constructor_errors_and_recovery():
    """Validate enum types, dates, scale, native maturity guards and supported inputs."""
    dynamic = cast(Any, Callability)
    with pytest.raises(TypeError):
        dynamic(100.0, 2, Date(1, 1, 2027))
    with pytest.raises(TypeError):
        dynamic(100.0, CallabilityType.Call, Date(1, 1, 2027), 2)
    for factory in [fixed, zero]:
        with pytest.raises(ItofinError):
            factory(settings(), [Callability(100.0, CallabilityType.Call, Date(1, 1, 2031))])
        with pytest.raises(ItofinError):
            factory(settings(), redemption=1e308)
        assert factory(settings()).notional() == 250.0
    with pytest.raises(TypeError):
        cast(Any, fixed)(settings(), payment_calendar=Calendar.null_calendar())
    with pytest.raises(TypeError):
        cast(Any, fixed)(settings(), first_period_day_counter=DayCounter.actual360())
    with pytest.raises(ItofinError):
        zero(settings(), maturity=Date(1, 1, 2045), calendar=Calendar.indonesia())
    with pytest.raises(ItofinError):
        fixed(settings(), ex_coupon_period=Period(2**31 - 1, "Years"))
    preceding = [Callability(100.0, CallabilityType.Put, Date(17, 1, 2027))]
    with pytest.raises(ItofinError):
        zero(settings(), preceding, payment_convention=BDC.Preceding)


@pytest.mark.parametrize("factory", [fixed, zero])
def test_zero_notional_callable_price_still_checks_native_engine(factory):
    """Do not route callable quotes through the straight-bond zero-notional shortcut."""
    policy = settings()
    bond = factory(policy)
    policy.set_evaluation_date(Date(15, 1, 2030) if factory is fixed else Date(18, 1, 2027))
    policy.set_include_reference_date_events(True)
    policy.set_include_todays_cash_flows(True)
    assert bond.notional() == 0.0
    assert not bond.is_expired()
    for action in [bond.dirty_price, bond.clean_price]:
        with pytest.raises(ItofinError, match="engine"):
            action()
        assert not bond.is_calculated()



def test_ex_coupon_period_policy_and_unadjusted_unused_calendar_horizon():
    """Match new-callable period/default policy without unused holiday lookups."""
    policy = settings()
    with pytest.raises(ItofinError, match="nonnegative"):
        fixed(policy, ex_coupon_period=Period(-1, "Days"))
    default_calendar = fixed(policy, ex_coupon_period=Period(7, "Days"))
    explicit_calendar = fixed(policy, ex_coupon_period=Period(7, "Days"), ex_coupon_calendar=Calendar.null_calendar())
    assert default_calendar.accrued_amount(Date(12, 1, 2027)) == explicit_calendar.accrued_amount(Date(12, 1, 2027))
    calendar = Calendar.indonesia()
    schedule = Schedule(Date(1, 1, 2044), Date(1, 1, 2045), Frequency.Annual, calendar, BDC.Unadjusted)
    callable_fixed = CallableFixedRateBond(0, 100.0, schedule, [0.04], DayCounter.actual360(), [], policy, payment_convention=BDC.Unadjusted, ex_coupon_period=Period(7, "Days"))
    assert callable_fixed.maturity_date() == Date(1, 1, 2045)
    assert callable_fixed.calendar().name == calendar.name
    callable_zero = zero(policy, calendar=calendar, maturity=Date(1, 1, 2045), payment_convention=BDC.Unadjusted)
    assert callable_zero.maturity_date() == Date(1, 1, 2045)
    with pytest.raises(ItofinError):
        CallableFixedRateBond(0, 100.0, schedule, [0.04], DayCounter.actual360(), [], policy, payment_convention=BDC.Following)
    with pytest.raises(ItofinError):
        zero(policy, calendar=calendar, maturity=Date(1, 1, 2045), payment_convention=BDC.Following)



def test_zero_generation_rule_retains_native_frequency_and_pure_analytics():
    """Construct core-valid Once-frequency schedules and check getter errors only on use."""
    native = json.loads((Path(__file__).parent / "fixtures/callable_foundation_zero_rule_native.json").read_text())
    policy = settings()
    schedule = Schedule(Date(15, 1, 2026), Date(15, 1, 2030), Frequency.Annual, Calendar.null_calendar(), BDC.Unadjusted, DateGeneration.Zero)
    bond = CallableFixedRateBond(2, 250.0, schedule, [0.045], DayCounter.actual360(), [], policy)
    assert isinstance(bond, Bond)
    assert not bond.is_calculated()
    with pytest.raises(ItofinError, match="frequency"):
        bond.frequency()
    assert bond.day_counter() == DayCounter.actual360()
    assert [flow.amount() for flow in bond.cashflows()] == pytest.approx([row[1] for row in native["flows"]], abs=2e-12)
    assert bond.clean_price_from_yield(0.04, DayCounter.actual360(), Compounding.Compounded, Frequency.Annual) == pytest.approx(native["clean_yield"], abs=2e-11, rel=2e-13)
    assert not bond.is_calculated()
    bond.freeze()
    with pytest.raises(ItofinError):
        bond.npv()
    assert not bond.is_calculated()
    bond.unfreeze()
    with pytest.raises(ItofinError, match="engine"):
        bond.npv()
    assert not bond.is_calculated()
