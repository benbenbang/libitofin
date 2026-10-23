"""Coupon-rate sensitivity is per100 and independent of signed flat-yield BPV."""
import csv
import datetime
import gc
import math
from pathlib import Path

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import Bond, CallableZeroCouponBond, FixedRateBond, ZeroCouponBond
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention as BDC, Calendar, Compounding, Date, DateGeneration, DayCounter, Frequency, Period, Schedule
from test_bond_analytics_bindings import floating_market


def market(face: float = 375.0) -> tuple[FixedRateBond, Settings, DayCounter, FlatForward, Date, list[Date]]:
    """Build an engine-free ordinary bond on a non-flat coupon schedule."""
    settings = Settings()
    dc, calendar = DayCounter.actual360(), Calendar.null_calendar()
    dates = [Date(24, 3, 2026), Date(24, 9, 2026), Date(24, 3, 2027), Date(24, 9, 2027), Date(24, 3, 2028)]
    schedule = Schedule(dates[0], dates[-1], Frequency.Semiannual, calendar, BDC.Unadjusted, DateGeneration.Backward)
    bond = FixedRateBond(5, face, schedule, [.047, .058], dc, settings, payment_convention=BDC.Unadjusted, redemption=102.0)
    curve = FlatForward(Date(1, 6, 2026), .032, DayCounter.actual365_fixed())
    return bond, settings, dc, curve, Date(26, 6, 2026), dates


@pytest.mark.parametrize("face", [100.0, 375.0, 750.0])
@pytest.mark.parametrize("rate", [-.01, 0.0, .052])
def test_per100_scaling_and_nonpar_atm(face: float, rate: float) -> None:
    """Independent coupon annuities pin units, discount dates and the own-NPV ATM target."""
    bond, settings, dc, curve, settlement, dates = market(face)
    curve_bps, yield_bps, coupons = 0.0, 0.0, 0.0
    for i, date in enumerate(dates[1:]):
        accrual = (date - dates[i]) / 360
        discount = math.exp(-.032 * (date - settlement) / 365)
        curve_bps += .01 * accrual * discount
        yield_bps += .01 * accrual * math.exp(-rate * (date - settlement) / 360)
        coupons += (.047 if i == 0 else .058) * accrual * discount
    args = rate, dc, Compounding.Continuous, Frequency.Annual
    assert bond.bps_from_curve(curve, settlement=settlement) == pytest.approx(curve_bps, abs=3e-12, rel=3e-13)
    assert bond.bps_from_yield(*args, settlement=settlement) == pytest.approx(yield_bps, abs=3e-12, rel=3e-13)
    assert bond.atm_rate_from_curve(curve, settlement=settlement) == pytest.approx(coupons / (curve_bps / .01), abs=3e-12, rel=3e-13)
    assert not bond.is_calculated()
    with pytest.raises(ItofinError):
        bond.bps_from_curve(curve)
    settings.set_evaluation_date(Date(19, 6, 2026))
    actual = bond.settlement_date()
    assert bond.bps_from_curve(curve) == bond.bps_from_curve(curve, settlement=actual)
    assert bond.bps_from_yield(*args) == bond.bps_from_yield(*args, settlement=actual)
    assert bond.atm_rate_from_curve(curve) == bond.atm_rate_from_curve(curve, settlement=actual)
    expected = bond.bps_from_curve(curve, settlement=settlement)
    del settings
    gc.collect()
    assert bond.bps_from_curve(curve, settlement=settlement) == expected


def test_domains_zero_bonds_and_callable_exclusion() -> None:
    """Recoverable boundary errors preserve support for genuine zero-coupon results."""
    bond, settings, dc, curve, settlement, dates = market()
    for rate in [math.nan, math.inf, -math.inf]:
        with pytest.raises(ItofinError):
            bond.bps_from_yield(rate, dc, Compounding.Compounded, Frequency.Annual, settlement=settlement)
    for date in [dates[-1], dates[-1] + 1]:
        with pytest.raises(ItofinError):
            bond.bps_from_curve(curve, settlement=date)
        with pytest.raises(ItofinError):
            bond.bps_from_yield(.03, dc, Compounding.Compounded, Frequency.Annual, settlement=date)
        with pytest.raises(ItofinError):
            bond.atm_rate_from_curve(curve, settlement=date)
    zero = ZeroCouponBond(0, Calendar.null_calendar(), 375.0, dates[-1], settings, payment_convention=BDC.Unadjusted)
    assert zero.bps_from_curve(curve, settlement=settlement) == 0.0
    assert zero.bps_from_yield(.03, dc, Compounding.Compounded, Frequency.Annual, settlement=settlement) == 0.0
    assert zero.atm_rate_from_curve(curve, settlement=settlement) == 0.0
    callable_bond = CallableZeroCouponBond(0, Calendar.null_calendar(), 375.0, dates[-1], dc, [], settings)
    with pytest.raises(ItofinError, match="ordinary"):
        callable_bond.bps_from_curve(curve, settlement=settlement)
    with pytest.raises(ItofinError, match="ordinary"):
        callable_bond.bps_from_yield(.03, dc, Compounding.Compounded, Frequency.Annual, settlement=settlement)
    with pytest.raises(ItofinError, match="ordinary"):
        callable_bond.atm_rate_from_curve(curve, settlement=settlement)


def test_floating_missing_fixings_are_not_needed_for_bps() -> None:
    """An amount-free BPS query must not inherit the ATM coupon-fixing guard."""
    bond, settings, dc, curve, _, _ = floating_market()
    settlement = Date(25, 6, 2026)
    initial = bond.bps_from_curve(curve, settlement=settlement)
    args = .052, dc, Compounding.Compounded, Frequency.Semiannual
    initial_yield = bond.bps_from_yield(*args, settlement=settlement)
    settings.set_evaluation_date(settlement)
    assert bond.bps_from_curve(curve, settlement=settlement) == initial
    assert bond.bps_from_yield(*args, settlement=settlement) == initial_yield
    with pytest.raises(ItofinError):
        bond.atm_rate_from_curve(curve, settlement=settlement)


def test_floating_live_amounts_and_frozen_cache() -> None:
    """ATM uses current coupons while analytics never mutates frozen engine results."""
    bond, settings, dc, curve, forward, vol = floating_market()
    cached = bond.npv()
    bond.freeze()
    settlement = Date(24, 6, 2026)
    before = bond.atm_rate_from_curve(curve, settlement=settlement)
    sensitivity = bond.bps_from_curve(curve, settlement=settlement)
    forward.set_value(.08)
    vol.set_value(.40)
    calculated = bond.is_calculated()
    assert bond.atm_rate_from_curve(curve, settlement=settlement) != before
    assert bond.bps_from_curve(curve, settlement=settlement) == sensitivity
    assert bond.is_calculated() == calculated
    assert bond.npv() == cached
    del settings, forward, vol, dc
    gc.collect()
    assert bond.bps_from_curve(curve, settlement=settlement) == sensitivity


def date_from_serial(serial: int) -> Date:
    """Convert the QuantLib/Excel serial using Python calendar arithmetic."""
    value = datetime.date(1899, 12, 30) + datetime.timedelta(days=serial)
    return Date(value.day, value.month, value.year)


def coupon_oracles() -> list[dict[str, str]]:
    """Read exact-binary64 text shared with the public Go regression."""
    with (Path(__file__).parent / "fixtures/bond_coupon_analytics.tsv").open(newline="") as stream:
        rows = list(csv.DictReader(stream, delimiter="\t"))
    assert len(rows) == 55
    return rows


@pytest.mark.parametrize("row", coupon_oracles(), ids=lambda row: row["name"])
def test_coupon_analytics_compiled_quantlib_oracles(row: dict[str, str]) -> None:
    """Pin every existing compounding/frequency combination plus zero and ex-coupon cases."""
    settings = Settings()
    dc, calendar = DayCounter.actual360(), Calendar.null_calendar()
    start, end, reference, settlement = [date_from_serial(int(row[key])) for key in ["start", "end", "reference", "settlement"]]
    face = float(row["face"])
    if row["kind"] == "zero":
        bond: Bond = ZeroCouponBond(5, calendar, face, end, settings, issue_date=start, payment_convention=BDC.Unadjusted, redemption=102.0)
    else:
        schedule = Schedule(start, end, Frequency.Semiannual, calendar, BDC.Unadjusted, DateGeneration.Backward)
        if row["kind"] == "ex_coupon":
            bond = FixedRateBond(5, face, schedule, [.047, .058], dc, settings, issue_date=start, payment_convention=BDC.Unadjusted, redemption=102.0, ex_coupon_period=Period(7, "Days"), ex_coupon_calendar=calendar)
        else:
            bond = FixedRateBond(5, face, schedule, [.047, .058], dc, settings, issue_date=start, payment_convention=BDC.Unadjusted, redemption=102.0)
    curve = FlatForward(reference, .032, DayCounter.actual365_fixed())
    modes = [Compounding.Simple, Compounding.Compounded, Compounding.Continuous, Compounding.SimpleThenCompounded, Compounding.CompoundedThenSimple]
    frequencies = [Frequency.Annual, Frequency.Semiannual, Frequency.Quarterly, Frequency.Monthly]
    expected = [float(row[key]) for key in ["bps_curve", "bps_yield", "atm_curve"]]
    got = [bond.bps_from_curve(curve, settlement=settlement), bond.bps_from_yield(float(row["yield"]), dc, modes[int(row["compounding"])], frequencies[int(row["frequency"])], settlement=settlement), bond.atm_rate_from_curve(curve, settlement=settlement)]
    assert got == pytest.approx(expected, abs=3e-12, rel=3e-13)
