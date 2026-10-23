"""Ordinary bond risk: independent analytical derivatives and retained native inputs."""
import gc
import datetime
import csv
from pathlib import Path
import math
from typing import Any

import pytest
from itofin import ItofinError, Settings
from itofin.instruments import CallableZeroCouponBond, Duration, FixedRateBond, ZeroCouponBond
from itofin.time import BusinessDayConvention as BDC, Calendar, Compounding, Date, DayCounter, Frequency, Period, Schedule, DateGeneration
from test_bond_analytics_bindings import floating_market


def zero(face=375., lag=5):
    """Construct an unpriced bond with no evaluation date."""
    settings = Settings()
    cal, dc = Calendar.null_calendar(), DayCounter.actual365_fixed()
    end, settlement = Date(2, 1, 2028), Date(2, 1, 2025)
    bond = ZeroCouponBond(lag, cal, face, end, settings, payment_convention=BDC.Unadjusted)
    return bond, settings, dc, settlement, (end - settlement) / 365.


def values(bond, rate, dc, mode, settlement):
    """Read every supported metric independently."""
    args = rate, dc, mode, Frequency.Semiannual
    options = {"settlement": settlement}
    result = [bond.duration(*args, duration_type=Duration.Simple, **options), bond.duration(*args, **options), bond.convexity(*args, **options), bond.basis_point_value(*args, **options), bond.yield_value_basis_point(*args, **options)]
    return result


@pytest.mark.parametrize("mode", [Compounding.Simple, Compounding.Compounded, Compounding.Continuous, Compounding.SimpleThenCompounded, Compounding.CompoundedThenSimple])
@pytest.mark.parametrize("rate", [-.01, 0., .052])
def test_zero_analytical_derivatives_and_unscaled_currency(mode, rate):
    """Closed-form one-payment derivatives independently pin signs and face scaling."""
    bond, settings, dc, settlement, t = zero()
    if mode in [Compounding.Simple, Compounding.CompoundedThenSimple]:
        discount, modified, convexity = 1 / (1 + rate*t), t / (1 + rate*t), 2*t*t / (1 + rate*t)**2
    elif mode == Compounding.Continuous:
        discount, modified, convexity = math.exp(-rate*t), t, t*t
    else:
        discount, modified, convexity = (1 + rate/2)**(-2*t), t / (1 + rate/2), t*(t+.5) / (1 + rate/2)**2
    price = 375 * discount
    expected = [t, modified, convexity, -modified*price*.0001 + convexity/100*price*.0001**2/2, .01 / (-price*modified)]
    assert values(bond, rate, dc, mode, settlement) == pytest.approx(expected, rel=2e-13, abs=3e-12)
    twice, _, _, _, _ = zero(750)
    scaled = values(twice, rate, dc, mode, settlement)
    assert scaled[:3] == pytest.approx(expected[:3], rel=2e-13, abs=3e-12)
    assert scaled[3:] == pytest.approx([expected[3]*2, expected[4]/2], rel=2e-13, abs=3e-12)
    args = rate, dc, mode, Frequency.Semiannual
    if mode == Compounding.Compounded:
        assert bond.duration(*args, duration_type=Duration.Macaulay, settlement=settlement) == pytest.approx(t)
    else:
        with pytest.raises(ItofinError):
            bond.duration(*args, duration_type=Duration.Macaulay, settlement=settlement)
    assert not bond.is_calculated()
    settings.set_evaluation_date(Date(28, 12, 2024))
    actual = bond.settlement_date()
    assert bond.convexity(*args) == bond.convexity(*args, settlement=actual)


def test_domains_retention_and_tradability():
    """Errors recover and explicit dates bypass lag but never maturity/tradability."""
    bond, settings, dc, settlement, _ = zero(lag=100000)
    args = .03, dc, Compounding.Compounded, Frequency.Semiannual
    with pytest.raises(ItofinError):
        bond.convexity(*args)
    expected = bond.convexity(*args, settlement=settlement)
    for bad in [math.nan, math.inf, -math.inf]:
        with pytest.raises(ItofinError):
            bond.convexity(bad, *args[1:], settlement=settlement)
    for bad_date in [Date(2, 1, 2028), Date(3, 1, 2028)]:
        with pytest.raises(ItofinError):
            bond.convexity(*args, settlement=bad_date)
    with pytest.raises(TypeError):
        dynamic_bond: Any = bond
        dynamic_bond.duration(*args, duration_type=99, settlement=settlement)
    del settings
    gc.collect()
    assert bond.convexity(*args, settlement=settlement) == expected
    callable_bond: Any = CallableZeroCouponBond(0, Calendar.null_calendar(), 375., Date(2, 1, 2028), DayCounter.actual365_fixed(), [], Settings())
    with pytest.raises(ItofinError, match="ordinary"):
        callable_bond.convexity(*args, settlement=settlement)


def test_floating_live_inputs_survive_frozen_engine_cache():
    """Analytical risk consumes current coupon data without invalidating frozen engine state."""
    bond, settings, dc, _, forward, vol = floating_market()
    cached = bond.npv()
    bond.freeze()
    settlement = Date(24, 6, 2026)
    initial = values(bond, .052, dc, Compounding.Compounded, settlement)
    assert bond.is_calculated()
    forward.set_value(.08)
    vol.set_value(.40)
    before = bond.is_calculated()
    changed = values(bond, .052, dc, Compounding.Compounded, settlement)
    assert changed != initial
    assert bond.is_calculated() == before
    assert bond.npv() == cached
    settings.set_include_reference_date_events(True)
    assert values(bond, .052, dc, Compounding.Compounded, settlement) == changed
    del forward, vol, settings
    gc.collect()
    assert values(bond, .052, dc, Compounding.Compounded, settlement) == changed


def risk_oracles():
    """Read the exact binary64 text table shared with Go."""
    with (Path(__file__).parent / "fixtures/bond_risk_oracle.tsv").open(newline="") as stream:
        rows = list(csv.DictReader(stream, delimiter="\t"))
    assert len(rows) == 55
    for row in rows:
        expected = {key: None if row[key] == "NA" else float(row[key]) for key in ["simple", "macaulay", "modified", "convexity", "basis_point_value", "yield_value_basis_point"]}
        yield {**row, **{key: int(row[key]) for key in ["start", "end", "settlement", "compounding", "frequency"]}, "face": float(row["face"]), "yield": float(row["yield"]), "expected": expected}


@pytest.mark.parametrize("row", list(risk_oracles()), ids=lambda row: row["name"])
def test_quantlib_wheel_risk_oracles(row):
    """Pin six independent scalar routes to fresh QuantLib 1.43 wheel queries."""
    def date(serial):
        value = datetime.date(1899, 12, 30) + datetime.timedelta(days=serial)
        return Date(value.day, value.month, value.year)
    settings, cal, dc = Settings(), Calendar.null_calendar(), DayCounter.actual360()
    if row["kind"] == "zero":
        bond = ZeroCouponBond(5, cal, row["face"], date(row["end"]), settings, payment_convention=BDC.Unadjusted, redemption=102., issue_date=date(row["start"]))
    else:
        schedule = Schedule(date(row["start"]), date(row["end"]), Frequency.Semiannual, cal, BDC.Unadjusted, DateGeneration.Backward)
        options = {"ex_coupon_period": Period(7, "Days"), "ex_coupon_calendar": cal} if row["kind"] == "ex_coupon" else {}
        bond = FixedRateBond(5, row["face"], schedule, [.047, .058], dc, settings, payment_convention=BDC.Unadjusted, redemption=102., issue_date=date(row["start"]), **options)
    mode = [Compounding.Simple, Compounding.Compounded, Compounding.Continuous, Compounding.SimpleThenCompounded, Compounding.CompoundedThenSimple][row["compounding"]]
    frequency = [Frequency.Annual, Frequency.Semiannual, Frequency.Quarterly, Frequency.Monthly][row["frequency"]]
    args, options = (row["yield"], dc, mode, frequency), {"settlement": date(row["settlement"])}
    for kind, key in [(Duration.Simple, "simple"), (Duration.Macaulay, "macaulay"), (Duration.Modified, "modified")]:
        if row["expected"][key] is None:
            with pytest.raises(ItofinError):
                bond.duration(*args, duration_type=kind, **options)
        else:
            assert bond.duration(*args, duration_type=kind, **options) == pytest.approx(row["expected"][key], abs=3e-12, rel=3e-13)
    for key in ["convexity", "basis_point_value", "yield_value_basis_point"]:
        assert getattr(bond, key)(*args, **options) == pytest.approx(row["expected"][key], abs=3e-12, rel=3e-13)
