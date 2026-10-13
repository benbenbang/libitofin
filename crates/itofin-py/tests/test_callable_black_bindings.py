"""Single-European relative-yield Black prices, pure IV and actual lazy lifecycle."""

import gc
import json
import math
from pathlib import Path
from typing import Any, cast

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import Bond, Callability, CallabilityType, CallableFixedRateBond, CallableZeroCouponBond, FixedRateBond
from itofin.models import HullWhite
from itofin.pricingengines import BlackCallableFixedRateBondEngine, DiscountingBondEngine, TreeCallableFixedRateBondEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import BondPriceType, FlatForward, ZeroCurve
from itofin.time import BusinessDayConvention as BDC, Calendar, Date, DateGeneration, DayCounter, Frequency, Schedule

NATIVE = json.loads((Path(__file__).parent / "fixtures/callable_black_native.json").read_text())


def date(value):
    """Date."""
    year, month, day = map(int, value.split("-"))
    return Date(day, month, year)


def policy(when="2026-10-12"):
    """Policy."""
    result = Settings()
    result.set_evaluation_date(date(when))
    return result


def curve(kind="flat", reference="2026-10-12", rate=0.035):
    """Curve."""
    dc = DayCounter.actual365_fixed()
    if kind == "nonflat":
        return ZeroCurve([date(d) for d in [reference, "2027-10-12", "2030-10-12", "2035-10-12"]], [0.015, 0.031, 0.024, 0.04], dc)
    return FlatForward(date(reference), rate, dc)


def sample(kind, settings, direction="call", face=1000.0, lag=2, exercise="2029-04-15", strike=None, rights=None, redemption=100.0, once=False, dc=None):
    """Sample."""
    dc = dc or DayCounter.actual365_fixed()
    strike = (100.0 if kind == "fixed" else 85.0) if strike is None else strike
    rights = [Callability(strike, CallabilityType.Call if direction == "call" else CallabilityType.Put, date(exercise))] if rights is None else rights
    if kind == "zero":
        return CallableZeroCouponBond(lag, Calendar.null_calendar(), face, date("2033-01-15"), dc, rights, settings, payment_convention=BDC.Unadjusted, redemption=redemption, issue_date=date("2026-01-15"))
    schedule = Schedule(date("2026-01-15"), date("2033-01-15"), Frequency.Annual if once else Frequency.Semiannual, Calendar.null_calendar(), BDC.Unadjusted, DateGeneration.Zero if once else DateGeneration.Backward)
    return CallableFixedRateBond(lag, face, schedule, [0.05], dc, rights, settings, payment_convention=BDC.Unadjusted, redemption=redemption, issue_date=date("2026-01-15"))


@pytest.mark.parametrize("row", NATIVE["black"])
def test_original_independent_black_currency_oracles(row):
    """Keep corrected settlement units and core's original absolute tolerances."""
    settings = policy("2026-01-15")
    dc = DayCounter.actual365_fixed()
    rights = [Callability(100.0, CallabilityType.Put if row["put"] else CallabilityType.Call, date("2028-01-15"))]
    if row["zero"]:
        bond = CallableZeroCouponBond(row["lag"], Calendar.null_calendar(), 1000.0, date("2031-01-15"), dc, rights, settings, payment_convention=BDC.Unadjusted, issue_date=date("2026-01-15"))
    else:
        schedule = Schedule(date("2026-01-15"), date("2031-01-15"), Frequency.Semiannual, Calendar.null_calendar(), BDC.Unadjusted)
        bond = CallableFixedRateBond(row["lag"], 1000.0, schedule, [0.055], dc, rights, settings, payment_convention=BDC.Unadjusted, issue_date=date("2026-01-15"))
    engine = BlackCallableFixedRateBondEngine(SimpleQuote(row["vol"]), curve(reference="2026-01-15", rate=0.04), settings)
    bond.set_black_engine(engine)
    assert isinstance(bond, Bond)
    assert not bond.is_calculated()
    assert bond.npv() == pytest.approx(row["npv"], abs=1e-8, rel=0)
    assert bond.settlement_value() == pytest.approx(row["settlement"], abs=1e-8, rel=0)
    assert bond.dirty_price() == pytest.approx(row["settlement"] / 10.0, abs=1e-8, rel=0)


@pytest.mark.parametrize("row", NATIVE["iv"])
def test_independent_clean_dirty_iv_targets_without_installed_engine(row):
    """Use independently corrected targets rather than a worker-only roundtrip."""
    settings = policy()
    bond = sample(row["product"], settings, row["direction"], row["face"], row["lag"])
    term = curve(row["curve"])
    for quote_type, label in [(BondPriceType.Clean, "clean"), (BondPriceType.Dirty, "dirty")]:
        root = bond.implied_volatility(row[f"corrected_{label}"], term, price_type=quote_type, max_evaluations=300)
        assert root == pytest.approx(row[f"corrected_iv_{label}"], abs=2e-8, rel=0)
        assert root == pytest.approx(row["vol"], abs=2e-8, rel=0)
        assert not bond.is_calculated()
    with pytest.raises(ItofinError):
        bond.npv()
    bond.set_black_engine(BlackCallableFixedRateBondEngine(SimpleQuote(row["vol"]), term, settings))
    assert bond.dirty_price() == pytest.approx(row["corrected_dirty"], abs=1e-8, rel=0)
    assert bond.clean_price() == pytest.approx(row["corrected_clean"], abs=1e-8, rel=0)
    if row["lag"] > 0:
        assert abs(bond.dirty_price() - row["dirty"]) > 1e-6


@pytest.mark.parametrize("kind", ["fixed", "zero"])
def test_live_black_graph_retention_freeze_replacement_and_recovery(kind):
    """Live black graph retention freeze replacement and recovery."""
    settings = policy()
    vol_a, rate_a = SimpleQuote(0.3), SimpleQuote(0.035)
    term_a = FlatForward.from_quote(date("2026-10-12"), rate_a, DayCounter.actual365_fixed())
    bond = sample(kind, settings)
    engine_a = BlackCallableFixedRateBondEngine(vol_a, term_a, settings)
    bond.set_black_engine(engine_a)
    del engine_a, term_a
    gc.collect()
    initial = bond.npv()
    bond.freeze()
    vol_a.set_value(0.7)
    assert bond.npv() == initial
    bond.recalculate()
    changed = bond.npv()
    assert changed < initial
    vol_a.set_value(0.2)
    assert bond.npv() == changed
    bond.unfreeze()
    assert bond.npv() > changed
    vol_a.set_value(math.nan)
    with pytest.raises(ItofinError):
        bond.npv()
    vol_a.set_value(0.3)
    assert bond.npv() == pytest.approx(initial, abs=1e-8, rel=0)
    vol_b, rate_b = SimpleQuote(0.15), SimpleQuote(0.04)
    term_b = FlatForward.from_quote(date("2026-10-12"), rate_b, DayCounter.actual365_fixed())
    bond.set_black_engine(BlackCallableFixedRateBondEngine(vol_b, term_b, settings))
    value_b = bond.npv()
    vol_a.set_value(0.8)
    rate_a.set_value(0.08)
    assert bond.is_calculated()
    assert bond.npv() == value_b
    vol_b.set_value(0.4)
    assert not bond.is_calculated()
    bond.npv()
    rate_b.set_value(0.05)
    assert not bond.is_calculated()
    model = HullWhite(curve(rate=0.03), 0.1, 0.01)
    bond.set_tree_engine(TreeCallableFixedRateBondEngine(model, 100, settings))
    tree_value = bond.npv()
    vol_b.set_value(0.9)
    rate_b.set_value(0.06)
    assert bond.is_calculated()
    assert bond.npv() == tree_value
    bond.set_black_engine(BlackCallableFixedRateBondEngine(SimpleQuote(0.25), curve(), settings))
    assert not bond.is_calculated()
    bond.npv()
    bond.freeze()
    settings.set_evaluation_date(date("2034-01-15"))
    assert bond.npv() == 0.0
    assert bond.clean_price() == 0.0
    with pytest.raises(ItofinError):
        bond.implied_volatility(0.0, curve())


@pytest.mark.parametrize("kind", ["fixed", "zero"])
@pytest.mark.parametrize("installed", ["none", "tree", "black"])
def test_pure_iv_keeps_installed_cache_and_freeze(kind, installed):
    """Pure iv keeps installed cache and freeze."""
    settings = policy()
    bond = sample(kind, settings, lag=7)
    rate = SimpleQuote(0.035)
    installed_curve = FlatForward.from_quote(date("2026-10-12"), rate, DayCounter.actual365_fixed())
    if installed == "tree":
        bond.set_tree_engine(TreeCallableFixedRateBondEngine(HullWhite(installed_curve, 0.1, 0.01), 100, settings))
    elif installed == "black":
        bond.set_black_engine(BlackCallableFixedRateBondEngine(SimpleQuote(0.2), installed_curve, settings))
    cached = bond.npv() if installed != "none" else None
    if installed != "none":
        bond.freeze()
    row = next(row for row in NATIVE["iv"] if row["product"] == kind and row["direction"] == "call" and row["lag"] == 7)
    explicit_curve = curve("nonflat")
    assert bond.implied_volatility(row["corrected_clean"], explicit_curve, max_evaluations=300) == pytest.approx(0.7, abs=2e-8, rel=0)
    for target in [math.nan, math.inf, -1e10, 1e10]:
        with pytest.raises(ItofinError):
            bond.implied_volatility(target, explicit_curve)
    with pytest.raises(ItofinError):
        bond.implied_volatility(row["corrected_clean"], explicit_curve, max_evaluations=1)
    assert bond.is_calculated() == (installed != "none")
    if installed != "none":
        rate.set_value(0.08)
        assert bond.npv() == cached
        bond.unfreeze()
        assert bond.npv() != cached
    else:
        with pytest.raises(ItofinError):
            bond.npv()


@pytest.mark.parametrize("kind", ["fixed", "zero"])
def test_exact_settings_guard_and_wrong_contracts(kind):
    """Exact settings guard and wrong contracts."""
    settings = policy()
    bond = sample(kind, settings)
    engine = BlackCallableFixedRateBondEngine(SimpleQuote(0.3), curve(), settings)
    bond.set_black_engine(engine)
    cached = bond.npv()
    for wrong in [BlackCallableFixedRateBondEngine(SimpleQuote(0.3), curve(), policy()), DiscountingBondEngine(curve(), settings)]:
        with pytest.raises(ItofinError):
            (bond.set_black_engine(wrong) if isinstance(wrong, BlackCallableFixedRateBondEngine) else bond.set_engine(wrong))
        assert bond.is_calculated()
        assert bond.npv() == cached
    with pytest.raises(TypeError):
        bond.set_black_engine(cast(Any, TreeCallableFixedRateBondEngine(HullWhite(curve(), 0.1, 0.01), 100, settings)))
    with pytest.raises(TypeError):
        BlackCallableFixedRateBondEngine(cast(Any, 0.3), curve(), settings)
    with pytest.raises(TypeError):
        bond.implied_volatility(100.0, cast(Any, engine))
    assert not hasattr(Bond, "set_black_engine")
    assert not hasattr(FixedRateBond, "implied_volatility")


@pytest.mark.parametrize("kind", ["fixed", "zero"])
@pytest.mark.parametrize("events", [[], ["2026-01-15"], ["2026-10-14"], ["2033-01-15"], ["2026-01-15", "2029-04-15"], ["2029-04-15", "2029-04-15"]])
def test_one_original_and_one_active_hard_right_required(kind, events):
    """One original and one active hard right required."""
    settings = policy()
    rights = [Callability(100.0, CallabilityType.Call, date(value)) for value in events]
    bond = sample(kind, settings, rights=rights)
    bond.set_black_engine(BlackCallableFixedRateBondEngine(SimpleQuote(0.3), curve(), settings))
    with pytest.raises(ItofinError):
        bond.npv()
    with pytest.raises(ItofinError):
        bond.implied_volatility(100.0, curve())
    settings.set_evaluation_date(date("2034-01-15"))
    assert bond.npv() == 0.0


@pytest.mark.parametrize("kind", ["fixed", "zero"])
@pytest.mark.parametrize("options", [{"accuracy": 0.0}, {"accuracy": -1.0}, {"accuracy": math.nan}, {"accuracy": math.inf}, {"max_evaluations": 0}, {"min_vol": -0.1}, {"min_vol": math.nan}, {"max_vol": math.inf}, {"max_vol": 0.0}, {"min_vol": 0.2, "max_vol": 0.2}])
def test_iv_controls_are_explicit_checked_inputs(kind, options):
    """Iv controls are explicit checked inputs."""
    bond = sample(kind, policy())
    with pytest.raises(ItofinError):
        bond.implied_volatility(100.0, curve(), **options)
    assert not bond.is_calculated()


@pytest.mark.parametrize("kind", ["fixed", "zero"])
def test_iv_endpoint_budgets_and_unsigned_extraction(kind):
    """Iv endpoint budgets and unsigned extraction."""
    settings = policy()
    bond = sample(kind, settings)
    term = curve()
    volatility = SimpleQuote(0.0)
    bond.set_black_engine(BlackCallableFixedRateBondEngine(volatility, term, settings))
    lower = bond.dirty_price()
    assert bond.implied_volatility(lower, term, price_type=BondPriceType.Dirty, max_evaluations=1) == 0.0
    volatility.set_value(1.0)
    upper = bond.dirty_price()
    with pytest.raises(ItofinError):
        bond.implied_volatility(upper, term, price_type=BondPriceType.Dirty, max_evaluations=1)
    assert bond.implied_volatility(upper, term, price_type=BondPriceType.Dirty, max_evaluations=2) == 1.0
    for budget in [-1, 2**200]:
        with pytest.raises(OverflowError):
            bond.implied_volatility(lower, term, max_evaluations=budget)
    with pytest.raises(TypeError):
        bond.implied_volatility(lower, term, price_type=cast(Any, 0))


def test_negative_clean_dirty_valid_and_exact_zero_yield():
    """Negative clean dirty valid and exact zero yield."""
    settings = policy()
    term = curve(rate=0.0)
    rights = [Callability(0.0, CallabilityType.Call, date("2026-11-15"), BondPriceType.Dirty)]
    bond = sample("fixed", settings, rights=rights)
    bond.set_black_engine(BlackCallableFixedRateBondEngine(SimpleQuote(0.3), term, settings))
    assert bond.dirty_price() == pytest.approx(0.0, abs=1e-12, rel=0)
    negative_clean = bond.clean_price()
    assert negative_clean < 0.0
    assert bond.implied_volatility(negative_clean, term, max_evaluations=1) == 0.0
    with pytest.raises(ItofinError):
        bond.implied_volatility(negative_clean, term, price_type=BondPriceType.Dirty)
    for direction in ["call", "put"]:
        zero = sample("zero", settings, direction)
        zero.set_black_engine(BlackCallableFixedRateBondEngine(SimpleQuote(0.7), term, settings))
        assert math.isfinite(zero.npv())
        assert zero.implied_volatility(zero.clean_price(), term, max_evaluations=1) == 0.0


@pytest.mark.parametrize("rate,vol,valid", [(1e-9, 0.3, True), (-1e-9, 0.3, False), (-0.01, 0.0, True), (-0.01, 0.3, False)])
def test_tiny_nonzero_and_negative_yields_are_not_zero_clamped(rate, vol, valid):
    """Tiny nonzero and negative yields are not zero clamped."""
    settings = policy()
    bond = sample("zero", settings)
    bond.set_black_engine(BlackCallableFixedRateBondEngine(SimpleQuote(vol), curve(rate=rate), settings))
    if valid:
        assert math.isfinite(bond.npv())
    else:
        with pytest.raises(ItofinError):
            bond.npv()


@pytest.mark.parametrize("volatility", [-0.1, math.nan, math.inf])
def test_invalid_live_market_is_deferred_until_pricing(volatility):
    """Invalid live market is deferred until pricing."""
    settings = Settings()
    engine = BlackCallableFixedRateBondEngine(SimpleQuote(volatility), curve(), settings)
    settings.set_evaluation_date(date("2026-10-12"))
    bond = sample("zero", settings)
    bond.set_black_engine(engine)
    with pytest.raises(ItofinError):
        bond.npv()


def test_native_once_schedule_prices_without_expanding_frequency_enum():
    """Native once schedule prices without expanding frequency enum."""
    settings = policy()
    bond = sample("fixed", settings, once=True, dc=DayCounter.actual360())
    assert isinstance(bond, CallableFixedRateBond)
    term = curve()
    with pytest.raises(ItofinError, match="frequency"):
        bond.frequency()
    bond.set_black_engine(BlackCallableFixedRateBondEngine(SimpleQuote(0.3), term, settings))
    target = bond.clean_price()
    assert math.isfinite(target)
    assert bond.implied_volatility(target, term, max_evaluations=300) == pytest.approx(0.3, abs=2e-8, rel=0)


@pytest.mark.parametrize("row", json.loads((Path(__file__).parent / "fixtures/callable_black_supplement.json").read_text())["rows"])
def test_independent_settlement_ex_coupon_indentures_and_volatility_clock(row):
    """Separate settlement ownership, exercise yield ownership and evaluation clock."""
    inputs, expected = row["inputs"], row["expected"]
    settings = policy(inputs["evaluation"])
    dc = DayCounter.actual360() if inputs["counter"] == "Actual360" else DayCounter.actual365_fixed()
    rights = [Callability(inputs["strike"], CallabilityType.Put if inputs["direction"] == "put" else CallabilityType.Call, date(inputs["exercise"]), BondPriceType.Clean if inputs["price_type"] == "Clean" else BondPriceType.Dirty)]
    if inputs["product"] == "zero":
        bond = CallableZeroCouponBond(inputs["lag"], Calendar.null_calendar(), inputs["face"], date(inputs["maturity"]), dc, rights, settings, payment_convention=BDC.Unadjusted, redemption=inputs["redemption"], issue_date=date(inputs["issue"]))
    else:
        from itofin.time import Period

        schedule = Schedule(date(inputs["issue"]), date(inputs["maturity"]), Frequency.Annual if inputs["once"] else Frequency.Semiannual, Calendar.null_calendar(), BDC.Unadjusted, DateGeneration.Zero if inputs["once"] else DateGeneration.Backward)
        bond = CallableFixedRateBond(inputs["lag"], inputs["face"], schedule, inputs["coupons"], dc, rights, settings, payment_convention=BDC.Unadjusted, redemption=inputs["redemption"], issue_date=date(inputs["issue"]), ex_coupon_period=Period(inputs["ex_days"], "Days"), ex_coupon_calendar=Calendar.null_calendar())
    term = curve(reference=inputs["reference"], rate=inputs["rate"])
    bond.set_black_engine(BlackCallableFixedRateBondEngine(SimpleQuote(inputs["vol"]), term, settings))
    assert bond.npv() == pytest.approx(expected["npv"], abs=1e-8, rel=0)
    assert bond.settlement_value() == pytest.approx(expected["settlement"], abs=1e-8, rel=0)
    assert bond.clean_price() == pytest.approx(expected["clean"], abs=1e-8, rel=0)
    assert bond.dirty_price() == pytest.approx(expected["dirty"], abs=1e-8, rel=0)
    assert bond.settlement_date() == date(row["components"]["settlement_date"])
    if "ex_at_settlement" not in inputs["name"]:
        assert bond.implied_volatility(expected["clean"], term, max_evaluations=300) == pytest.approx(inputs["vol"], abs=2e-8, rel=0)


@pytest.mark.parametrize("kind", ["fixed", "zero"])
@pytest.mark.parametrize("face", [1e-100, 100.0, 1234567.89, 1e306])
def test_native_iv_normalizes_quote_residual_across_supported_face_scales(kind, face):
    """Native iv normalizes quote residual across supported face scales."""
    row = next(row for row in NATIVE["iv"] if row["product"] == kind and row["direction"] == "call" and row["lag"] == 0)
    bond = sample(kind, policy(), face=face, lag=0)
    root = bond.implied_volatility(row["corrected_dirty"], curve(), price_type=BondPriceType.Dirty, max_evaluations=300)
    assert root == pytest.approx(0.3, abs=2e-8, rel=0)


@pytest.mark.parametrize("kind", ["fixed", "zero"])
def test_calendar_preflight_no_evaluation_issue_floor_and_overflow(kind):
    """Calendar preflight no evaluation issue floor and overflow."""
    settings = Settings()
    bond = sample(kind, settings)
    with pytest.raises(ItofinError):
        bond.implied_volatility(100.0, curve())
    settings.set_evaluation_date(date("2199-12-31"))
    with pytest.raises(ItofinError):
        bond.implied_volatility(100.0, curve())
    assert not bond.is_calculated()
    settings.set_evaluation_date(date("2025-12-01"))
    assert bond.settlement_date() == date("2026-01-15")


def test_calendar_horizon_is_checked_before_native_iv_traversal():
    """Calendar horizon is checked before native iv traversal."""
    settings = policy("2045-01-01")
    dc = DayCounter.actual365_fixed()
    bond = CallableZeroCouponBond(2, Calendar.indonesia(), 100.0, date("2046-01-01"), dc, [Callability(90.0, CallabilityType.Put, date("2045-07-01"))], settings, payment_convention=BDC.Unadjusted)
    with pytest.raises(ItofinError):
        bond.implied_volatility(90.0, curve(reference="2045-01-01"))
    assert not bond.is_calculated()
