"""Bond valuation, ownership and boundary checks against native QuantLib fixtures."""

# standard library
import datetime
import gc
import json
import math
from typing import Any, cast
from pathlib import Path

# third party
import pytest

# itofin library
from itofin import ItofinError, Settings
from itofin.instruments import Bond, FixedRateBond, ZeroCouponBond
from itofin.pricingengines import DiscountingBondEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention as BDC, Calendar, Date, DayCounter, Frequency, Period, Schedule

ROWS = json.loads((Path(__file__).parent / "fixtures/bond_bindings_native.json").read_text())["rows"]


def date(value):
    """Build a facade date from a native fixture tuple."""
    return Date(*value)


def serial(value):
    """Compute the native date serial independently."""
    return (datetime.date(value.year, value.month, value.day) - datetime.date(1899, 12, 30)).days


def fixed(settings, face=250.0, coupons=None, **options):
    """Build the common annual fixed-rate fixture."""
    schedule = Schedule(Date(15, 1, 2026), Date(15, 1, 2030), Frequency.Annual, Calendar.null_calendar(), BDC.Unadjusted)
    return FixedRateBond(2, face, schedule, coupons or [0.045], DayCounter.actual360(), settings, **options)


def zero(settings, face=250.0, maturity=None, calendar=None, **options):
    """Build the common adjusted zero-coupon fixture."""
    return ZeroCouponBond(2, calendar or Calendar.weekends_only(), face, maturity or Date(17, 1, 2027), settings, **options)


def engine(settings, quote=None, include=None):
    """Build a retained discounting engine with optional live quote."""
    curve = FlatForward.from_quote(Date(15, 3, 2026), quote, DayCounter.actual360()) if quote else FlatForward(Date(15, 3, 2026), 0.031, DayCounter.actual360())
    return DiscountingBondEngine(curve, settings, include)


@pytest.mark.parametrize("row", ROWS, ids=[f'{r["kind"]}-{r["when"]}-{r["redemption"]}' for r in ROWS])
def test_native_fixtures(row):
    """Match native currency values, quotes, dates and complete cash-flow legs."""
    settings = Settings()
    settings.set_evaluation_date(date(row["when"]))
    options = {"redemption": row["redemption"], "issue_date": date(row["issue"]) if row["issue"] else None}
    if row["kind"] == "fixed":
        if row["ex_days"] is not None:
            options.update(ex_coupon_period=Period(row["ex_days"], "Days"), ex_coupon_calendar=Calendar.null_calendar())
        bond = fixed(settings, row["face"], row["coupons"], **options)
        assert bond.frequency() == Frequency.Annual
        assert bond.day_counter() == DayCounter.actual360()
        assert bond.first_period_day_counter() is None
    else:
        bond = zero(settings, row["face"], **options)
    assert isinstance(bond, Bond)
    bond.set_engine(engine(settings))
    values = [bond.npv(), bond.settlement_value(), bond.clean_price(), bond.dirty_price(), bond.accrued_amount(), bond.notional()]
    assert values == pytest.approx(row["values"], abs=2e-11, rel=2e-13)
    assert serial(bond.settlement_date()) == row["settlement"]
    assert serial(bond.maturity_date()) == row["maturity"]
    assert bond.is_expired() == row["expired"]
    assert bond.settlement_days() == 2
    flows = bond.cashflows()
    assert len(flows) == len(row["flows"])
    for index, expected in enumerate(row["flows"]):
        flow = flows[index]
        assert serial(flow.date()) == expected[0]
        assert flow.amount() == pytest.approx(expected[1], abs=2e-12)
    assert len(bond.redemptions()) == 1
    assert bond.issue_date() == (date(row["issue"]) if row["issue"] else None)


def test_retention_live_quotes_settings_and_freezing():
    """Retain owners and refresh quotes while preserving ordinary freeze behavior."""
    settings = Settings()
    settings.set_evaluation_date(Date(15, 3, 2026))
    quote = SimpleQuote(0.031)
    bond = fixed(settings)
    pricing = engine(settings, quote)
    bond.set_engine(pricing)
    del pricing
    gc.collect()
    assert not bond.is_calculated()
    initial = bond.npv()
    assert bond.is_calculated()
    retained_leg = bond.cashflows()
    bond.freeze()
    quote.set_value(0.08)
    assert bond.npv() == initial
    bond.recalculate()
    assert bond.npv() < initial
    recalculated = bond.npv()
    quote.set_value(0.01)
    assert bond.npv() == recalculated
    bond.unfreeze()
    assert bond.npv() > initial
    settings.set_evaluation_date(Date(16, 1, 2030))
    assert bond.is_expired()
    assert bond.npv() == bond.settlement_value() == bond.clean_price() == bond.dirty_price() == 0.0
    del bond, quote, settings
    gc.collect()
    assert retained_leg[-1].amount() == 250.0


@pytest.mark.parametrize("override", [None, False, True])
def test_same_day_flags_refresh_cached_values_but_honor_freeze(override):
    """Respect today-flow overrides and expiry-before-freeze native semantics."""
    settings = Settings()
    settings.set_evaluation_date(Date(17, 1, 2027))
    curve = FlatForward(Date(17, 1, 2027), 0.03, DayCounter.actual360())
    bond = ZeroCouponBond(0, Calendar.null_calendar(), 250.0, Date(17, 1, 2027), settings, redemption=102.0)
    bond.set_engine(DiscountingBondEngine(curve, settings, override))
    assert bond.npv() == 0.0
    settings.set_include_todays_cash_flows(True)
    settings.set_include_reference_date_events(True)
    assert settings.include_reference_date_events()
    assert bond.npv() == 255.0
    assert bond.settlement_value() == 255.0
    assert bond.notional() == bond.dirty_price() == bond.clean_price() == 0.0
    bond.freeze()
    settings.set_include_todays_cash_flows(False)
    assert bond.npv() == 0.0
    bond.unfreeze()
    assert bond.npv() == 0.0


@pytest.mark.parametrize("face", [1e-250, 1e300])
def test_extreme_face_normalized_quotes_remain_finite(face):
    """Keep normalized quotes invariant across tiny and huge face amounts."""
    settings = Settings()
    settings.set_evaluation_date(Date(15, 3, 2026))
    bond = zero(settings, face)
    bond.set_engine(engine(settings))
    pay = datetime.date(2027, 1, 18)
    settle = datetime.date(2026, 3, 17)
    expected = 100.0 * math.exp(-0.031 * (pay - settle).days / 360.0)
    assert bond.dirty_price() == pytest.approx(expected, rel=3e-14)
    assert bond.clean_price() == bond.dirty_price()


@pytest.mark.parametrize("value", [0.0, -1.0, math.inf, -math.inf, math.nan])
def test_invalid_monetary_inputs_are_typed_errors(value):
    """Reject invalid monetary inputs without poisoning healthy constructions."""
    settings = Settings()
    with pytest.raises(ItofinError):
        fixed(settings, value)
    with pytest.raises(ItofinError):
        zero(settings, redemption=value)
    settings.set_evaluation_date(Date(15, 3, 2026))
    healthy = zero(settings)
    healthy.set_engine(engine(settings))
    assert healthy.npv() > 0.0


def test_missing_engine_settings_mismatch_issue_floor_and_date_overflow():
    """Surface missing engines, settings mismatch and invalid date advances."""
    settings = Settings()
    settings.set_evaluation_date(Date(15, 3, 2026))
    bond = zero(settings, issue_date=Date(1, 4, 2026))
    assert bond.settlement_date() == Date(1, 4, 2026)
    with pytest.raises(ItofinError, match="engine"):
        bond.npv()
    with pytest.raises(ItofinError, match="share settings"):
        bond.set_engine(engine(Settings()))
    bond.set_engine(engine(settings))
    assert bond.npv() > 0.0
    settings.set_evaluation_date(Date(31, 12, 2199))
    with pytest.raises(ItofinError, match="range"):
        bond.settlement_date()
    with pytest.raises(ItofinError):
        ZeroCouponBond(2**32 - 1, Calendar.null_calendar(), 100.0, Date(1, 1, 2027), settings)
    with pytest.raises(TypeError):
        cast(Any, Bond)()
    with pytest.raises(TypeError):
        cast(Any, Bond)(object())


def test_constructor_preserves_options_and_rejects_bad_coupon_or_payment_overflow():
    """Preserve first-period options and reject nonfinite coupons and boundaries."""
    settings = Settings()
    bond = fixed(settings, first_period_day_counter=DayCounter.actual365_fixed())
    assert bond.first_period_day_counter() == DayCounter.actual365_fixed()
    with pytest.raises(ItofinError):
        fixed(settings, coupons=[math.nan])
    with pytest.raises(ItofinError):
        zero(settings, 1e308, redemption=1e308)
    with pytest.raises(ItofinError):
        zero(settings, maturity=Date(1, 1, 2045), calendar=Calendar.indonesia())


def test_reference_flags_refresh_unexpired_bond_and_obey_engine_override():
    """Refresh silent event flags, respecting engine overrides and unexpired freeze."""
    settings = Settings()
    settings.set_evaluation_date(Date(15, 1, 2027))
    curve = FlatForward(Date(15, 1, 2027), 0.031, DayCounter.actual360())
    live = fixed(settings)
    excluded = fixed(settings)
    live.set_engine(DiscountingBondEngine(curve, settings))
    excluded.set_engine(DiscountingBondEngine(curve, settings, False))
    original = live.npv()
    excluded_price = excluded.npv()
    settings.set_include_reference_date_events(True)
    coupon = live.cashflows()[0].amount()
    assert live.npv() == pytest.approx(original + coupon, abs=2e-12)
    assert excluded.npv() == excluded_price
    live.freeze()
    settings.set_include_reference_date_events(False)
    assert live.npv() == pytest.approx(original + coupon, abs=2e-12)
    live.unfreeze()
    assert live.npv() == original


def test_overflow_safe_quoted_price_uses_scaled_ratio():
    """Recover a representable quote when currency scaling overflows."""
    settings = Settings()
    settings.set_evaluation_date(Date(15, 3, 2026))
    bond = zero(settings, 1e306)
    curve = FlatForward(Date(15, 3, 2026), -2.0, DayCounter.actual360())
    bond.set_engine(DiscountingBondEngine(curve, settings))
    assert math.isfinite(bond.settlement_value())
    assert math.isinf(bond.settlement_value() * 100.0)
    expected = 100.0 * math.exp(2.0 * (datetime.date(2027, 1, 18) - datetime.date(2026, 3, 17)).days / 360.0)
    assert bond.dirty_price() == pytest.approx(expected, rel=3e-14)
