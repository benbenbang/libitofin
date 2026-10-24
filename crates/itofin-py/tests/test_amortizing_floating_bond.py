"""Plain ibor amortization, exact balances and retained interleaved guards."""

import gc
import math
from concurrent.futures import ThreadPoolExecutor
from typing import Callable, TypedDict

import pytest

from itofin import ItofinError, Settings
from itofin.cashflows import BlackIborCouponPricer
from itofin.indexes import Currency, CustomIborIndex, IborIndex
from itofin.instruments import AmortizingFloatingRateBond, Bond, FloatingRateBond
from itofin.pricingengines import DiscountingBondEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import BondPriceType, FlatForward
from itofin.time import BusinessDayConvention as BDC, Calendar, Compounding, Date, DayCounter, Frequency, Period, Schedule


class BondOptions(TypedDict, total=False):
    fixing_days: int
    spreads: list[float]
    issue_date: Date
    ex_coupon_period: Period


class BondConfig(BondOptions):
    settlement_days: int
    notionals: list[float]
    schedule: Schedule
    index: IborIndex
    accrual_day_counter: DayCounter
    settings: Settings


class BondChanges(TypedDict, total=False):
    settlement_days: int
    fixing_days: int
    spreads: list[float]
    settings: Settings
    issue_date: Date
    ex_coupon_period: Period
    index: IborIndex


def changed(config: BondConfig, values: BondChanges) -> BondConfig:
    """Copy construction terms for intentionally invalid runtime probes."""
    result = config.copy()
    if "settlement_days" in values:
        result["settlement_days"] = values["settlement_days"]
    if "fixing_days" in values:
        result["fixing_days"] = values["fixing_days"]
    if "spreads" in values:
        result["spreads"] = values["spreads"]
    if "settings" in values:
        result["settings"] = values["settings"]
    if "issue_date" in values:
        result["issue_date"] = values["issue_date"]
    if "ex_coupon_period" in values:
        result["ex_coupon_period"] = values["ex_coupon_period"]
    if "index" in values:
        result["index"] = values["index"]
    return result


def near(actual, expected):
    assert math.isfinite(actual) and math.isfinite(expected)
    assert actual == pytest.approx(expected, abs=3e-11, rel=3e-13)


def market(*, limited=False):
    settings = Settings()
    today = Date(28, 9, 2026)
    settings.set_evaluation_date(today)
    calendar, dc = Calendar.null_calendar(), DayCounter.actual360()
    quote = SimpleQuote(0.04)
    forward = FlatForward.from_quote(today, quote, dc)
    discount = FlatForward(today, 0.025, dc)
    if limited:
        index = CustomIborIndex("amortizing-bound", Period(6, "Months"), 2, Currency.eur(), calendar, calendar, Calendar.china(), BDC.Following, False, dc, forward, settings)
    else:
        index = IborIndex("amortizing-plain", Period(6, "Months"), 2, Currency.eur(), calendar, BDC.Following, False, dc, forward, settings)
    schedule = Schedule(Date(5, 10, 2026), Date(5, 10, 2027), Frequency.Quarterly, calendar, BDC.Unadjusted)
    config = BondConfig(settlement_days=0, notionals=[1000.0, 800.0, 800.0, 500.0], schedule=schedule, index=index, accrual_day_counter=dc, settings=settings)
    return config, quote, discount


def test_amortizing_balances_principal_copy_and_live_discounting():
    config, quote, discount = market()
    balances = config["notionals"]
    bond = AmortizingFloatingRateBond(**config)
    assert isinstance(bond, Bond)
    pricer = BlackIborCouponPricer()
    bond.set_coupon_pricer(pricer)
    del pricer
    bond.set_engine(DiscountingBondEngine(discount, config["settings"]))
    balances[:] = [1.0]
    flows = bond.cashflows()
    assert len(flows) == 7
    for position, principal in [(1, 200.0), (4, 300.0), (6, 500.0)]:
        near(flows[position].amount(), principal)
    for position in [0, 2, 3, 5]:
        assert math.isfinite(flows[position].amount())
        assert flows[position].amount() > 0.0
    near(bond.notional(Date(4, 1, 2027)), 1000.0)
    near(bond.notional(Date(5, 1, 2027)), 800.0)
    near(bond.notional(Date(5, 7, 2027)), 500.0)
    near(bond.notional(Date(5, 10, 2027)), 0.0)
    initial = bond.npv()
    assert math.isfinite(initial)
    quote.set_value(0.06)
    assert math.isfinite(bond.npv()) and bond.npv() > initial
    bond.freeze()
    frozen = bond.npv()
    quote.set_value(0.03)
    near(bond.npv(), frozen)
    bond.unfreeze()
    assert bond.npv() < frozen
    del config, discount
    gc.collect()
    assert math.isfinite(bond.npv())


def test_principal_guards_are_none_after_interleaving_and_owner_drop():
    config, quote, discount = market(limited=True)
    settings = config["settings"]
    bond = AmortizingFloatingRateBond(**config)
    index, leg, principals = bond.index(), bond.cashflows(), bond.redemptions()
    coupons = [leg[i] for i in [0, 2, 3, 5]]
    principal_views = [leg[i] for i in [1, 4, 6]]
    del bond, config, quote, discount, leg
    gc.collect()
    for flow, expected in zip(principal_views, [200.0, 300.0, 500.0]):
        near(flow.amount(), expected)
    for flow in coupons:
        with pytest.raises(ItofinError, match="2026"):
            flow.amount()
    settings.set_evaluation_date(Date(10, 10, 2027))
    starts = [Date(5, 10, 2026), Date(5, 1, 2027), Date(5, 4, 2027), Date(5, 7, 2027), Date(5, 10, 2027)]
    for i, (flow, notional) in enumerate(zip(coupons, [1000.0, 800.0, 800.0, 500.0])):
        with pytest.raises(ItofinError):
            flow.amount()
        index.add_fixing(starts[i] - 2, 0.037)
        near(flow.amount(), notional * 0.037 * DayCounter.actual360().year_fraction(starts[i], starts[i + 1]))
    del index, settings
    gc.collect()
    for i, expected in enumerate([200.0, 300.0, 500.0]):
        near(principals[i].amount(), expected)


def test_fixing_today_forecast_then_history_and_missing_past():
    config, quote, _ = market()
    settings = config["settings"]
    settings.set_evaluation_date(Date(3, 10, 2026))
    bond = AmortizingFloatingRateBond(**config)
    first = bond.cashflows()[0]
    initial = first.amount()
    assert math.isfinite(initial)
    quote.set_value(0.06)
    updated = first.amount()
    assert math.isfinite(updated) and updated > initial
    settings.set_evaluation_date(Date(4, 10, 2026))
    with pytest.raises(ItofinError):
        first.amount()
    bond.index().add_fixing(Date(3, 10, 2026), 0.037)
    expected = 1000.0 * 0.037 * DayCounter.actual360().year_fraction(Date(5, 10, 2026), Date(5, 1, 2027))
    near(first.amount(), expected)
    quote.set_value(0.08)
    near(first.amount(), expected)


def test_contractual_maturity_precedes_adjusted_final_payment():
    config, _, _ = market()
    config["schedule"] = Schedule(Date(31, 1, 2027), Date(31, 10, 2027), Frequency.Quarterly, Calendar.weekends_only(), BDC.Unadjusted)
    config["notionals"] = [1000.0, 700.0, 500.0]
    bond = AmortizingFloatingRateBond(**config)
    assert bond.maturity_date() == Date(31, 10, 2027)
    assert bond.redemptions()[-1].date() == Date(1, 11, 2027)
    near(bond.notional(Date(31, 10, 2027)), 500.0)
    near(bond.notional(Date(1, 11, 2027)), 0.0)


def test_ex_coupon_filters_coupon_but_not_principal_and_accrual_uses_history():
    config, _, _ = market()
    settings = config["settings"]
    settings.set_evaluation_date(Date(30, 12, 2026))
    discount = FlatForward(Date(30, 12, 2026), 0.025, config["accrual_day_counter"])
    bond = AmortizingFloatingRateBond(**changed(config, {"ex_coupon_period": Period(7, "Days")}), ex_coupon_calendar=Calendar.null_calendar())
    bond.set_engine(DiscountingBondEngine(discount, settings))
    assert math.isfinite(bond.npv())
    near(bond.cashflows()[1].amount(), 200.0)
    with pytest.raises(ItofinError):
        bond.accrued_amount()
    bond.index().add_fixing(Date(3, 10, 2026), 0.037)
    near(bond.accrued_amount(), -100.0 * 0.037 * 6.0 / 360.0)


@pytest.mark.parametrize("balances", [[], [1000.0], [1000.0] * 5, [1000.0, 0.0, 800.0, 500.0], [1000.0, -1.0, 800.0, 500.0], [1000.0, 1100.0, 800.0, 500.0], [1000.0, math.nan, 800.0, 500.0], [math.inf, 800.0, 800.0, 500.0]])
def test_exact_positive_finite_nonincreasing_balances(balances):
    config, _, _ = market()
    config["notionals"] = balances
    with pytest.raises(ItofinError):
        AmortizingFloatingRateBond(**config)


def test_invalid_terms_settings_identity_and_plain_only_contract():
    config, _, _ = market()
    invalid_changes: list[BondChanges] = [{"spreads": [math.nan]}, {"spreads": [0.0] * 5}, {"settings": Settings()}, {"issue_date": Date(5, 1, 2027)}, {"fixing_days": 2**32 - 1}, {"settlement_days": 2**32 - 1}, {"ex_coupon_period": Period(-1, "Days")}]
    for changes in invalid_changes:
        with pytest.raises(ItofinError):
            AmortizingFloatingRateBond(**changed(config, changes))
    dynamic_factory: Callable[..., AmortizingFloatingRateBond] = AmortizingFloatingRateBond
    for changes in [dict(caps=[0.04]), dict(floors=[0.0]), dict(redemption=99.0), dict(gearings=[2.0])]:
        with pytest.raises(TypeError):
            dynamic_factory(**(config | changes))
    bullet = FloatingRateBond(0, 1000.0, config["schedule"], config["index"], config["accrual_day_counter"], config["settings"])
    assert len(bullet.redemptions()) == 1


def test_python_unsendable_ownership():
    config, _, _ = market()
    bond = AmortizingFloatingRateBond(**config)
    def other_thread():
        try:
            bond.notional()
        except BaseException as error:
            return str(error)
        return "unexpected success"
    with ThreadPoolExecutor(max_workers=1) as pool:
        assert "thread" in pool.submit(other_thread).result().lower()


def test_date_span_bounds_return_typed_errors():
    config, _, _ = market()
    bound = Date(31, 12, 2199) - Date(1, 1, 1901)
    invalid_changes: list[BondChanges] = [{"settlement_days": bound + 1}, {"fixing_days": bound + 1}, {"settlement_days": 2**32 - 1}, {"fixing_days": 2**32 - 1}]
    for changes in invalid_changes:
        with pytest.raises(ItofinError, match="supported date range"):
            AmortizingFloatingRateBond(**changed(config, changes))
    index = IborIndex("OversizedAmortizingIndex", Period(6, "Months"), bound + 1, Currency.usd(), Calendar.null_calendar(), BDC.Following, False, config["accrual_day_counter"], None, config["settings"])
    with pytest.raises(ItofinError, match="supported date range"):
        AmortizingFloatingRateBond(**changed(config, {"index": index, "fixing_days": 0}))


def test_existing_bullet_zero_notional_analytics_keep_nontradable_errors():
    config, _, _ = market()
    settings, counter = config["settings"], config["accrual_day_counter"]
    settings.set_evaluation_date(Date(6, 10, 2027))
    bullet = FloatingRateBond(0, 1000.0, config["schedule"], config["index"], counter, settings)
    near(bullet.notional(), 0.0)
    for kind in [BondPriceType.Clean, BondPriceType.Dirty]:
        with pytest.raises(ItofinError, match="non tradable at .* settlement date"):
            bullet.yield_rate(0.0, counter, Compounding.Compounded, Frequency.Semiannual, price_type=kind)
    for read in [bullet.clean_price_from_yield, bullet.dirty_price_from_yield]:
        with pytest.raises(ItofinError, match="non tradable at .* settlement date"):
            read(0.0, counter, Compounding.Compounded, Frequency.Semiannual)
