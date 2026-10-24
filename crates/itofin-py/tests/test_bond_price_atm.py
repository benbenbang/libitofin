"""Explicit price targets preserve the existing own-NPV bond ATM route."""

import gc
import math

import pytest

from itofin import ItofinError
from itofin.instruments import CallableZeroCouponBond
from itofin.termstructures import BondPriceType, FlatForward
from itofin.time import Calendar, Date, DayCounter
from test_bond_analytics_bindings import floating_market
from test_bond_coupon_analytics_bindings import market


def near(actual: float, expected: float) -> None:
    """Require finite values before the inherited strict analytic tolerance."""
    assert math.isfinite(actual) and math.isfinite(expected)
    assert actual == pytest.approx(expected, abs=3e-12, rel=3e-13)


@pytest.mark.parametrize("face", [100.0, 375.0, 750.0])
@pytest.mark.parametrize("target", [-20.0, 80.0, 100.0, 110.0])
def test_price_target_closed_form_and_clean_dirty(face: float, target: float) -> None:
    """Coupon annuity and principal PV independently fix per-100 price units."""
    bond, settings, dc, curve, settlement, dates = market(face)
    annuity = sum(
        100 * (d - dates[i]) / 360 * math.exp(-0.032 * (d - settlement) / 365) for i, d in enumerate(dates[1:])
    )
    principal = 102 * math.exp(-0.032 * (dates[-1] - settlement) / 365)
    accrued = 100 * 0.047 * (settlement - dates[0]) / 360
    near(bond.atm_rate_from_price(curve, target, settlement=settlement), (target + accrued - principal) / annuity)
    near(
        bond.atm_rate_from_price(curve, target + accrued, price_type=BondPriceType.Dirty, settlement=settlement),
        (target + accrued - principal) / annuity,
    )
    own = bond.atm_rate_from_curve(curve, settlement=settlement)
    quote = bond.clean_price_from_curve(curve, settlement=settlement)
    near(bond.atm_rate_from_price(curve, quote, settlement=settlement), own)
    assert not bond.is_calculated()
    settings.set_evaluation_date(Date(19, 6, 2026))
    actual = bond.settlement_date()
    near(bond.atm_rate_from_price(curve, target), bond.atm_rate_from_price(curve, target, settlement=actual))
    del settings, dc
    gc.collect()
    near(bond.atm_rate_from_price(curve, target, settlement=settlement), (target + accrued - principal) / annuity)


def test_price_target_errors_and_callable_exclusion() -> None:
    """Domain errors cannot masquerade as successful nonfinite results."""
    bond, settings, dc, curve, settlement, dates = market()
    for price in [math.nan, math.inf, -math.inf, float.fromhex("0x1.fffffffffffffp+1023")]:
        with pytest.raises(ItofinError):
            bond.atm_rate_from_price(curve, price, settlement=settlement)
    with pytest.raises(ItofinError):
        bond.atm_rate_from_price(curve, 100)
    for day in [dates[-1], dates[-1] + 1]:
        with pytest.raises(ItofinError):
            bond.atm_rate_from_price(curve, 100, settlement=day)
    callable_bond = CallableZeroCouponBond(0, Calendar.null_calendar(), 375.0, dates[-1], dc, [], settings)
    with pytest.raises(ItofinError, match="ordinary"):
        callable_bond.atm_rate_from_price(curve, 100, settlement=settlement)


def test_price_target_live_discount_and_floating_fixing_frozen_state() -> None:
    """Dirty targets still read guarded live coupons without changing cached NPV."""
    bond, settings, dc, curve, forward, vol = floating_market()
    settlement = Date(26, 6, 2026)
    cached = bond.npv()
    bond.freeze()
    before = bond.atm_rate_from_price(curve, 100, price_type=BondPriceType.Dirty, settlement=settlement)
    clean = bond.atm_rate_from_price(curve, 100, settlement=settlement)
    assert math.isfinite(clean)
    forward.set_value(0.08)
    vol.set_value(0.40)
    settings.set_include_reference_date_events(True)
    calculated = bond.is_calculated()
    near(bond.atm_rate_from_price(curve, 100, price_type=BondPriceType.Dirty, settlement=settlement), before)
    changed_clean = bond.atm_rate_from_price(curve, 100, settlement=settlement)
    assert math.isfinite(changed_clean)
    assert changed_clean != clean
    assert bond.is_calculated() == calculated
    near(bond.npv(), cached)
    other = FlatForward(Date(1, 6, 2026), 0.08, DayCounter.actual365_fixed())
    changed_discount = bond.atm_rate_from_price(other, 100, price_type=BondPriceType.Dirty, settlement=settlement)
    assert math.isfinite(changed_discount)
    assert changed_discount != before
    settings.set_evaluation_date(Date(25, 6, 2026))
    for price_type in [BondPriceType.Clean, BondPriceType.Dirty]:
        with pytest.raises(ItofinError):
            bond.atm_rate_from_price(curve, 100, price_type=price_type, settlement=Date(25, 6, 2026))
