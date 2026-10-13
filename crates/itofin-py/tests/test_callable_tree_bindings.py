"""Hull-White callable tree native prices, cache wiring and typed attachment."""

import gc
import json
import math
from pathlib import Path
from typing import Any, cast

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import Bond, Callability, CallabilityType, CallableFixedRateBond, CallableZeroCouponBond, FixedRateBond
from itofin.models import HullWhite
from itofin.pricingengines import DiscountingBondEngine, TreeCallableFixedRateBondEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import BondPriceType, FlatForward, ZeroCurve
from itofin.time import BusinessDayConvention as BDC, Calendar, Compounding, Date, DateGeneration, DayCounter, Frequency, Period, Schedule

NATIVE = json.loads((Path(__file__).parent / "fixtures/callable_tree_native.json").read_text())
CASES = [(family, row) for family in ["fixed", "ex_coupon", "zero"] for row in NATIVE[family]["rows"]]


def date(value):
    """Build a facade date from an independent ISO fixture value."""
    year, month, day = map(int, value.split("-"))
    return Date(day, month, year)


def policy(when="2026-07-07"):
    """Build an explicit shared evaluation-date allocation."""
    result = Settings()
    result.set_evaluation_date(date(when))
    return result


def event(when, direction, quote, amount):
    """Use named conventions rather than copying QuantLib enum ordinals."""
    return Callability(amount, CallabilityType.Call if direction in ["Call", "C", "call"] else CallabilityType.Put, date(when), BondPriceType.Clean if quote.lower() == "clean" else BondPriceType.Dirty)


def events(text):
    """Read native zero-fixture ordered exercise values."""
    return [event(when, direction, quote, float(amount)) for when, direction, quote, amount in [entry.split(",") for entry in text.split(";") if entry]]


def zero_curve():
    """Build the independent nonflat zero curve without extrapolation."""
    inputs = NATIVE["zero"]["inputs"]
    return ZeroCurve([date(value) for value in inputs["curve_dates"]], inputs["curve_yields"], DayCounter.actual365_fixed())


def sample(kind, settings, rights=None, curve=None):
    """Build a typed callable and retained tree engine for lifecycle probes."""
    day_counter = DayCounter.actual365_fixed()
    if kind == "fixed":
        schedule = Schedule(date("2026-07-07"), date("2031-07-07"), Frequency.Semiannual, Calendar.null_calendar(), BDC.Unadjusted)
        bond = CallableFixedRateBond(2, 1000.0, schedule, [0.05], day_counter, rights or [], settings, payment_convention=BDC.Unadjusted)
    else:
        bond = CallableZeroCouponBond(2, Calendar.null_calendar(), 1000.0, date("2031-07-07"), day_counter, rights or [], settings, payment_convention=BDC.Unadjusted)
    curve = curve or FlatForward(date("2026-07-07"), 0.03, day_counter)
    model = HullWhite(curve, 0.1, 0.01)
    engine = TreeCallableFixedRateBondEngine(model, 100, settings)
    return bond, engine


@pytest.mark.parametrize("family,row", CASES, ids=[f"{family}-{row.get('name', row.get('case'))}" for family, row in CASES])
def test_independent_native_fixed_ex_coupon_and_nonflat_zero_prices(family, row):
    """Retain exact preexisting core oracle tolerances across inherited quote methods."""
    inputs = NATIVE[family]["inputs"]
    if family == "zero":
        settings = policy(inputs["reference"])
        curve = FlatForward(date(inputs["reference"]), -0.012, DayCounter.actual365_fixed()) if row["case"] == "negative_rates" else zero_curve()
        model = HullWhite(curve, inputs["a"], inputs["sigma"])
        bond = CallableZeroCouponBond(inputs["settlement_days"], Calendar.target(), inputs["face"], date(inputs["maturity"]), DayCounter.actual365_fixed(), events(row["events"]), settings, payment_convention=BDC.Following if row["convention"] == "Following" else BDC.Preceding, redemption=row["redemption"], issue_date=date(inputs["issue"]))
        engine = TreeCallableFixedRateBondEngine(model, inputs["tree_steps"], settings)
        currency_tolerance, quote_tolerance = 2e-8, 2e-12
        expected_settlement = row["settlement_date"]
        expected_accrued = 0.0
        if row["case"] == "zero_redemption_put":
            assert row["npv"] > 0.0
    else:
        reference = row.get("reference", inputs.get("reference", inputs.get("issue")))
        settings = policy(reference)
        day_counter = DayCounter.actual365_fixed()
        curve = FlatForward(date(reference), inputs["risk_free"], day_counter)
        model = HullWhite(curve, inputs["hull_white_a"], inputs["hull_white_sigma"])
        issue = inputs.get("issue", inputs.get("reference"))
        schedule = Schedule(date(issue), date(inputs["maturity"]), Frequency.Semiannual, Calendar.null_calendar(), BDC.Unadjusted)
        if family == "ex_coupon":
            rights = [event(item["date"], item["kind"], item["quote_type"], item["price"]) for item in row["events"]]
            options = {"ex_coupon_period": Period(row["ex_coupon_days"], "Days"), "ex_coupon_calendar": Calendar.null_calendar()}
        else:
            rights = [] if row["name"] == "noncallable" else [event(row["call_date"], "put" if row["name"] == "put_clean" else "call", "dirty" if "dirty" in row["name"] else "clean", row["call_price"])]
            options = {}
        bond = CallableFixedRateBond(inputs["settlement_days"], inputs["face"], schedule, [inputs["coupon"]], day_counter, rights, settings, payment_convention=BDC.Unadjusted, issue_date=date(issue), **options)
        engine = TreeCallableFixedRateBondEngine(model, inputs["tree_steps"], settings)
        currency_tolerance, quote_tolerance = 1e-10, 1e-11
        expected_settlement = row["settlement"]
        expected_accrued = row["accrued"]
    assert isinstance(bond, Bond)
    assert not bond.is_calculated()
    bond.set_tree_engine(engine)
    assert not bond.is_calculated()
    assert bond.npv() == pytest.approx(row["npv"], abs=currency_tolerance, rel=0.0)
    assert bond.settlement_value() == pytest.approx(row["settlement_value"], abs=currency_tolerance, rel=0.0)
    assert bond.dirty_price() == pytest.approx(row["dirty"], abs=quote_tolerance, rel=0.0)
    assert bond.clean_price() == pytest.approx(row["clean"], abs=quote_tolerance, rel=0.0)
    assert bond.accrued_amount() == pytest.approx(expected_accrued, abs=1e-14, rel=0.0)
    assert bond.settlement_date() == date(expected_settlement)
    assert bond.is_calculated()


@pytest.mark.parametrize("row", NATIVE["supplement"]["rows"], ids=lambda row: row["name"])
def test_aligned_direction_filtering_lags_and_native_once_schedule(row):
    """Use a fresh eligible-only native schedule oracle instead of its known index defect."""
    if row["kind"] == "zero":
        settings = policy("2026-10-12")
        bond = CallableZeroCouponBond(row["settlement_days"], Calendar.target(), 1234567.89, date("2031-10-12"), DayCounter.actual365_fixed(), [event(*entry) for entry in row["original_events"]], settings, redemption=107.5, issue_date=date("2026-10-09"))
        engine = TreeCallableFixedRateBondEngine(HullWhite(zero_curve(), 0.073, 0.014), 120, settings)
        expected = row["expected_aligned"]
        currency_tolerance, quote_tolerance = 2e-8, 2e-12
        if row["settlement_days"] > 0:
            assert abs(expected["clean"] - row["native_original"]["clean"]) > 1.0
    else:
        settings = policy("2026-03-15")
        day_counter = DayCounter.actual360()
        schedule = Schedule(date("2026-01-15"), date("2030-01-15"), Frequency.Annual, Calendar.null_calendar(), BDC.Unadjusted, DateGeneration.Zero)
        bond = CallableFixedRateBond(2, 250.0, schedule, [0.045], day_counter, [], settings)
        engine = TreeCallableFixedRateBondEngine(HullWhite(FlatForward(date("2026-03-15"), 0.031, day_counter), 0.1, 0.01), 100, settings)
        with pytest.raises(ItofinError, match="frequency"):
            bond.frequency()
        expected = row["expected"]
        currency_tolerance, quote_tolerance = 1e-10, 1e-11
    bond.set_tree_engine(engine)
    assert bond.npv() == pytest.approx(expected["npv"], abs=currency_tolerance, rel=0.0)
    assert bond.settlement_value() == pytest.approx(expected["settlement_value"], abs=currency_tolerance, rel=0.0)
    assert bond.clean_price() == pytest.approx(expected["clean"], abs=quote_tolerance, rel=0.0)
    assert bond.dirty_price() == pytest.approx(expected["dirty"], abs=quote_tolerance, rel=0.0)


@pytest.mark.parametrize("kind", ["fixed", "zero"])
def test_live_quote_retention_freeze_recalculate_and_engine_replacement(kind):
    """Share actual retained dependencies and detach the former engine on replacement."""
    settings = policy()
    old_quote = SimpleQuote(0.03)
    old_curve = FlatForward.from_quote(date("2026-07-07"), old_quote, DayCounter.actual365_fixed())
    bond, engine = sample(kind, settings, curve=old_curve)
    bond.set_tree_engine(engine)
    del old_curve, engine
    gc.collect()
    initial = bond.npv()
    assert bond.is_calculated()
    bond.freeze()
    old_quote.set_value(0.06)
    assert bond.npv() == initial
    bond.recalculate()
    repriced = bond.npv()
    assert repriced < initial
    old_quote.set_value(0.015)
    assert bond.npv() == repriced
    bond.unfreeze()
    assert bond.npv() > initial
    new_quote = SimpleQuote(0.04)
    new_curve = FlatForward.from_quote(date("2026-07-07"), new_quote, DayCounter.actual365_fixed())
    replacement = TreeCallableFixedRateBondEngine(HullWhite(new_curve, 0.1, 0.01), 100, settings)
    bond.set_tree_engine(replacement)
    expected, expected_engine = sample(kind, settings, curve=new_curve)
    expected.set_tree_engine(expected_engine)
    assert bond.npv() == expected.npv()
    assert bond.is_calculated()
    old_quote.set_value(0.11)
    assert bond.is_calculated()
    before = bond.npv()
    new_quote.set_value(0.02)
    assert not bond.is_calculated()
    assert bond.npv() > before
    leg = bond.cashflows()
    del replacement, new_curve, expected, expected_engine, new_quote
    gc.collect()
    assert bond.npv() > 0.0
    bond.freeze()
    settings.set_evaluation_date(date("2031-07-08"))
    del settings
    gc.collect()
    assert bond.is_expired()
    assert bond.npv() == bond.settlement_value() == bond.clean_price() == bond.dirty_price() == 0.0
    del bond
    gc.collect()
    assert leg[-1].amount() == 1000.0


@pytest.mark.parametrize("kind", ["fixed", "zero"])
def test_engine_settings_wrong_types_and_failed_pricing_recovery(kind):
    """Validate exact allocation identity and retain a healthy installed engine on failure."""
    settings = policy()
    bond, engine = sample(kind, settings)
    bond.set_tree_engine(engine)
    cached = bond.npv()
    wrong_settings = policy()
    _, mismatch = sample(kind, wrong_settings)
    with pytest.raises(ItofinError, match="share settings"):
        bond.set_tree_engine(mismatch)
    assert bond.is_calculated()
    assert bond.npv() == cached
    discount = DiscountingBondEngine(FlatForward(date("2026-07-07"), 0.03, DayCounter.actual365_fixed()), settings)
    with pytest.raises(ItofinError, match="noncallable"):
        bond.set_engine(discount)
    with pytest.raises(TypeError):
        cast(Any, bond).set_tree_engine(discount)
    with pytest.raises(TypeError):
        cast(Any, bond).set_engine(engine)
    assert bond.is_calculated()
    assert bond.npv() == cached
    future_curve = FlatForward(date("2026-08-01"), 0.03, DayCounter.actual365_fixed())
    failing = TreeCallableFixedRateBondEngine(HullWhite(future_curve, 0.1, 0.01), 100, settings)
    bond.set_tree_engine(failing)
    with pytest.raises(ItofinError, match="settlement before"):
        bond.npv()
    assert not bond.is_calculated()
    bond.set_tree_engine(engine)
    assert bond.npv() == cached


@pytest.mark.parametrize("kind", ["fixed", "zero"])
def test_pure_inherited_analytics_do_not_change_live_callable_cache(kind):
    """Contractual-cashflow analytical success/errors preserve the actual typed cache."""
    settings = policy()
    rights = [Callability(75.0, CallabilityType.Call, date("2028-07-07"))]
    bond, engine = sample(kind, settings, rights)
    bond.set_tree_engine(engine)
    cached = bond.npv()
    bond.freeze()
    curve = FlatForward(date("2026-07-07"), 0.03, DayCounter.actual365_fixed())
    assert bond.clean_price_from_curve(curve) > bond.clean_price()
    yield_price = bond.clean_price_from_yield(0.04, DayCounter.actual365_fixed(), Compounding.Compounded, Frequency.Annual)
    assert bond.yield_rate(yield_price, DayCounter.actual365_fixed(), Compounding.Compounded, Frequency.Annual) == pytest.approx(0.04, abs=1e-10)
    with pytest.raises(ItofinError):
        bond.clean_price_from_yield(math.nan, DayCounter.actual365_fixed(), Compounding.Compounded, Frequency.Annual)
    assert bond.is_calculated()
    assert bond.npv() == cached
    bond.unfreeze()
    assert bond.npv() == cached


def test_positive_steps_unset_evaluation_typed_receivers_and_same_day_zero_flags():
    """Retain checked constructor and native expiry/policy contracts without eager pricing."""
    settings = Settings()
    curve = FlatForward(date("2026-07-07"), 0.03, DayCounter.actual365_fixed())
    model = HullWhite(curve, 0.1, 0.01)
    with pytest.raises(ItofinError, match="positive"):
        TreeCallableFixedRateBondEngine(model, 0, settings)
    for steps in [-1, 2**128]:
        with pytest.raises(OverflowError):
            TreeCallableFixedRateBondEngine(model, steps, settings)
    engine = TreeCallableFixedRateBondEngine(model, 100, settings)
    bond = CallableZeroCouponBond(0, Calendar.null_calendar(), 1000.0, date("2027-07-07"), DayCounter.actual365_fixed(), [], settings, payment_convention=BDC.Unadjusted)
    bond.set_tree_engine(engine)
    with pytest.raises(ItofinError, match="evaluation date"):
        bond.npv()
    settings.set_evaluation_date(date("2027-07-07"))
    assert bond.npv() == 0.0
    settings.set_include_reference_date_events(True)
    settings.set_include_todays_cash_flows(True)
    assert not bond.is_calculated()
    assert bond.npv() > 0.0
    assert bond.notional() == bond.clean_price() == bond.dirty_price() == 0.0
    bond.freeze()
    settings.set_include_reference_date_events(False)
    settings.set_include_todays_cash_flows(False)
    assert not bond.is_calculated()
    assert bond.npv() == 0.0
    schedule = Schedule(date("2026-07-07"), date("2031-07-07"), Frequency.Semiannual, Calendar.null_calendar(), BDC.Unadjusted)
    plain = FixedRateBond(2, 1000.0, schedule, [0.05], DayCounter.actual365_fixed(), settings)
    with pytest.raises(AttributeError):
        cast(Any, plain).set_tree_engine(engine)
    with pytest.raises(TypeError):
        cast(Any, CallableFixedRateBond).set_tree_engine(plain, engine)
