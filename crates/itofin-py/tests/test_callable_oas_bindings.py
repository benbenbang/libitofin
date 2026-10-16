"""Pure explicit-tree OAS with native settlement-normalized QuantLib references."""

import gc
import json
import math
from pathlib import Path

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import Callability, CallabilityType, CallableFixedRateBond, CallableZeroCouponBond
from itofin.models import HullWhite
from itofin.pricingengines import TreeCallableFixedRateBondEngine
from itofin.termstructures import BondPriceType, DiscountCurve
from itofin.time import BusinessDayConvention as BDC, Calendar, Compounding, Date, DayCounter, Frequency, Schedule

ORACLE = json.loads((Path(__file__).parents[3] / "sdk/go/testdata/callable_oas_oracle.json").read_text())


def sample(kind, lag=2, settings=None, evaluate=True):
    """Reproduce independent source inputs with real retained ownership."""
    settings = settings or Settings()
    reference = Date(7, 7, 2026)
    if evaluate:
        settings.set_evaluation_date(reference)
    dc = DayCounter.actual365_fixed()
    curve = DiscountCurve([reference, Date(7, 7, 2027), Date(7, 7, 2029), Date(7, 7, 2036)], [1.0, 0.983, 0.915, 0.67], dc)
    model = HullWhite(curve, 0.1, 0.01)
    tree = TreeCallableFixedRateBondEngine(model, 100, settings)
    rights = [Callability(100.0, CallabilityType.Call, Date(7, 8, 2028)), Callability(97.0, CallabilityType.Put, Date(7, 1, 2029), BondPriceType.Dirty)]
    if kind == "fixed":
        schedule = Schedule(reference, Date(7, 7, 2031), Frequency.Semiannual, Calendar.null_calendar(), BDC.Unadjusted)
        bond = CallableFixedRateBond(lag, 12345.0, schedule, [0.05], dc, rights, settings, payment_convention=BDC.Unadjusted, issue_date=reference)
    else:
        bond = CallableZeroCouponBond(lag, Calendar.null_calendar(), 12345.0, Date(7, 7, 2031), dc, rights, settings, payment_convention=BDC.Unadjusted, issue_date=reference)
    return bond, tree, dc, settings


@pytest.mark.parametrize("row", ORACLE["cases"], ids=[f"{r['kind']}-{r['lag']}-{r['compounding']}" for r in ORACLE["cases"]])
def test_thirty_independent_native_quotes_and_roots(row):
    """Preserve native absolute quote and spread tolerance without relative slack."""
    assert ORACLE["quantlib_version"] == "1.43"
    assert len(ORACLE["cases"]) == 30
    bond, tree, _, _ = sample(row["kind"], row["lag"])
    dc = DayCounter.actual365_fixed() if row["day_counter"] == 365 else DayCounter.actual360()
    comp = getattr(Compounding, row["compounding"])
    assert not bond.is_calculated()
    price = bond.clean_price_oas(row["spread"], tree, dc, comp, Frequency.Semiannual)
    assert price == pytest.approx(row["normalized_clean"], abs=2e-10, rel=0.0)
    assert bond.oas(price, tree, dc, comp, Frequency.Semiannual) == pytest.approx(row["spread"], abs=2e-10, rel=0.0)
    year, month, day = map(int, row["settlement_date"].split("-"))
    settlement = Date(day, month, year)
    assert bond.clean_price_oas(row["spread"], tree, dc, comp, Frequency.Semiannual, settlement) == price
    assert not bond.is_calculated()


@pytest.mark.parametrize("kind", ["fixed", "zero"])
def test_immutable_frozen_state_budget_failures_and_retention(kind):
    """Successful and failed probes retain the installed native lazy result."""
    bond, tree, dc, settings = sample(kind)
    bond.set_tree_engine(tree)
    old = bond.npv()
    bond.freeze()
    args = (tree, dc, Compounding.Continuous, Frequency.Annual)
    price = bond.clean_price_oas(0.015, *args)
    assert bond.oas(price, *args, guess=0.0) == pytest.approx(0.015, abs=2e-10, rel=0.0)
    for controls in [{"max_evaluations": 1}, {"max_evaluations": 0}, {"accuracy": 0.0}, {"accuracy": math.nan}, {"guess": math.inf}]:
        with pytest.raises(ItofinError):
            bond.oas(price, *args, **controls)
    assert bond.is_calculated()
    assert bond.npv() == old
    bond.unfreeze()
    del settings
    gc.collect()
    assert bond.clean_price_oas(0.015, *args) == price


@pytest.mark.parametrize("kind", ["fixed", "zero"])
def test_actual_settlement_and_settings_allocation_identity(kind):
    """An actual nonbusiness date is neither adjusted nor evaluation-lagged."""
    bond, tree, dc, settings = sample(kind, 10)
    _, other, _, _ = sample(kind, 10)
    args = (dc, Compounding.Compounded, Frequency.Semiannual)
    with pytest.raises(ItofinError, match="share settings"):
        bond.clean_price_oas(0.01, other, *args)
    actual = Date(11, 7, 2026)
    first = bond.clean_price_oas(0.01, tree, *args, actual)
    settings.set_evaluation_date(Date(15, 7, 2026))
    assert bond.clean_price_oas(0.01, tree, *args, actual) == first
    assert bond.clean_price_oas(0.01, tree, *args) != first
    unset_bond, unset_tree, _, _ = sample(kind, 10, evaluate=False)
    assert unset_bond.clean_price_oas(0.01, unset_tree, *args, actual) == first
    with pytest.raises(ItofinError):
        unset_bond.clean_price_oas(0.01, unset_tree, *args)
    for invalid in [math.nan, math.inf, -math.inf]:
        with pytest.raises(ItofinError):
            bond.clean_price_oas(invalid, tree, *args, actual)
    for invalid in [Date(6, 7, 2026), Date(7, 7, 2031)]:
        with pytest.raises(ItofinError):
            bond.clean_price_oas(0.01, tree, *args, invalid)
