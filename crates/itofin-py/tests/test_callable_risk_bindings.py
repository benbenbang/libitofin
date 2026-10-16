"""Explicit immutable callable risk, QuantLib 1.43 clean-normalized oracle."""
import csv
import gc
import math
from pathlib import Path

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import Callability, CallabilityType, CallableFixedRateBond, CallableZeroCouponBond
from itofin.models import HullWhite
from itofin.pricingengines import TreeCallableFixedRateBondEngine
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention as BDC, Calendar, Compounding, Date, DayCounter, Frequency, Schedule

ROWS = list(csv.DictReader((Path(__file__).parent / "fixtures/callable_risk_native.tsv").read_text().splitlines(), delimiter="\t"))


def fixture(kind, right="none", redemption=100.0):
    """Use the exact native fixture inputs, not a facade-derived reference."""
    settings = Settings()
    reference, issue, maturity = Date(12, 10, 2026), Date(7, 7, 2026), Date(7, 7, 2031)
    settings.set_evaluation_date(reference)
    dc, cal = DayCounter.actual365_fixed(), Calendar.null_calendar()
    rights = [] if right == "none" else [Callability(100.0, CallabilityType.Call if right == "call" else CallabilityType.Put, Date(7, 7, 2028))]
    if kind == "fixed":
        schedule = Schedule(issue, maturity, Frequency.Semiannual, cal, BDC.Unadjusted)
        bond = CallableFixedRateBond(0, 1000.0, schedule, [0.05], dc, rights, settings, payment_convention=BDC.Unadjusted, issue_date=issue)
    else:
        bond = CallableZeroCouponBond(0, cal, 1000.0, maturity, dc, rights, settings, payment_convention=BDC.Unadjusted, redemption=redemption, issue_date=issue)
    model = HullWhite(FlatForward(reference, 0.03, dc), 0.1, 0.01)
    return bond, TreeCallableFixedRateBondEngine(model, 100, settings), dc, settings, model


@pytest.mark.parametrize("row", ROWS)
def test_quantlib143_clean_normalized_duration_convexity(row):
    """Preserve original core oracle absolute tolerances, including negative convexity."""
    bond, engine, dc, _, _ = fixture(row["kind"], row["right"])
    comp, freq = {"continuous": (Compounding.Continuous, Frequency.Annual), "compounded": (Compounding.Compounded, Frequency.Semiannual), "simple": (Compounding.Simple, Frequency.Annual)}[row["compounding"]]
    bump = float(row["bump"])
    args = (0.0075, engine, dc, comp, freq)
    for name, tolerance in [("effective_duration", 2e-9), ("effective_convexity", 2e-6)]:
        value = getattr(bond, name)(*args, bump=bump)
        assert value == pytest.approx(float(row[name.removeprefix("effective_")]), abs=tolerance, rel=0)
        if bump == 2e-4:
            assert getattr(bond, name)(*args) == value
    assert not bond.is_calculated()
    with pytest.raises(ItofinError):
        bond.npv()


@pytest.mark.parametrize("kind", ["fixed", "zero"])
@pytest.mark.parametrize("method", ["effective_duration", "effective_convexity"])
def test_risk_validation_identity_and_frozen_cache(kind, method):
    """Pure errors and successful probes leave a frozen cached installed engine unchanged."""
    bond, engine, dc, settings, model = fixture(kind, "call")
    args = [0.0075, engine, dc, Compounding.Continuous, Frequency.Annual]
    read = getattr(bond, method)
    expected = read(*args)
    bond.set_tree_engine(engine)
    cached = bond.npv()
    bond.freeze()
    wrong = Settings()
    wrong.set_evaluation_date(Date(12, 10, 2026))
    wrong_engine = TreeCallableFixedRateBondEngine(model, 100, wrong)
    for spread in [math.nan, math.inf, -math.inf]:
        with pytest.raises(ItofinError):
            read(spread, *args[1:])
    for bump in [0.0, -1.0, math.nan, math.inf, 1e-320]:
        with pytest.raises(ItofinError):
            read(*args, bump=bump)
    with pytest.raises(ItofinError):
        read(args[0], wrong_engine, *args[2:])
    assert read(*args) == expected
    del engine, model
    gc.collect()
    assert bond.is_calculated()
    assert bond.npv() == cached
    settings.set_include_reference_date_events(True)
    assert read(*args) == expected
    assert bond.npv() == cached


def test_zero_center_preserves_native_success_and_failed_bump():
    """A zero center still must validate bumped conversions, with no fabricated early zero."""
    bond, engine, dc, _, _ = fixture("zero", redemption=0.0)
    for name in ["effective_duration", "effective_convexity"]:
        read = getattr(bond, name)
        assert read(0.0, engine, dc, Compounding.Continuous, Frequency.Annual) == 0.0
        with pytest.raises(ItofinError):
            read(0.0, engine, dc, Compounding.Simple, Frequency.Annual, bump=10.0)


@pytest.mark.parametrize("kind", ["fixed", "zero"])
def test_uncached_frozen_and_unsendable_risk(kind):
    """An immutable risk query neither warms a frozen base nor changes thread ownership."""
    from concurrent.futures import ThreadPoolExecutor

    bond, engine, dc, _, _ = fixture(kind)
    bond.freeze()
    args = (0.0, engine, dc, Compounding.Continuous, Frequency.Annual)
    assert math.isfinite(bond.effective_duration(*args))
    assert math.isfinite(bond.effective_convexity(*args))
    assert not bond.is_calculated()
    with ThreadPoolExecutor(max_workers=1) as pool:
        with pytest.raises(BaseException, match="unsendable|thread"):
            pool.submit(bond.effective_duration, *args).result()
    assert not bond.is_calculated()
