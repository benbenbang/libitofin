"""Price-fed facades consume independently executed QuantLib and Decimal facts."""

import csv
import json
import math
from pathlib import Path
from typing import Any

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import Bond, FixedRateBond, ZeroCouponBond
from itofin.termstructures import BondPriceType, FlatForward
from itofin.time import (
    BusinessDayConvention as BDC,
    Calendar,
    Date,
    DateGeneration,
    DayCounter,
    Frequency,
    Period,
    Schedule,
)

FIXTURES = Path(__file__).parent / "fixtures"
RECORDS = [json.loads(line) for line in (FIXTURES / "bond_price_atm_cases.jsonl").read_text().splitlines()]
CASES = {row["value"]["id"]: row["value"] for row in RECORDS if row["record_type"] == "case"}
with (FIXTURES / "bond_atm_from_price_reference.csv").open(newline="") as stream:
    ROWS = [row for row in csv.DictReader(stream) if row["kind"] != "amortizing"]
with (FIXTURES / "bond_atm_from_price_errors.csv").open(newline="") as stream:
    ERRORS = [row for row in csv.DictReader(stream) if row["kind"] != "amortizing"]
assert len(CASES) == 49 and len(ROWS) == 182 and len(ERRORS) == 40


def date(value: str) -> Date:
    """Convert the fixture's explicit civil dates."""
    year, month, day = map(int, value.split("-"))
    return Date(day, month, year)


def build(case: dict[str, Any]) -> tuple[Bond, FlatForward, Date]:
    """Only existing fixed/zero factories, without silently claiming amortization."""
    settings, calendar, counter = Settings(), Calendar.null_calendar(), DayCounter.actual360()
    dates = [date(d) for d in case["dates"]]
    bond: Bond
    if case["kind"] == "zero":
        bond = ZeroCouponBond(
            case["settlement_days"],
            calendar,
            case["balances"][0],
            dates[-1],
            settings,
            payment_convention=BDC.Unadjusted,
            redemption=case["redemption"],
            issue_date=dates[0],
        )
    else:
        assert case["kind"] == "fixed"
        schedule = Schedule(
            dates[0], dates[-1], Frequency.Semiannual, calendar, BDC.Unadjusted, DateGeneration.Backward
        )
        bond = FixedRateBond(
            case["settlement_days"],
            case["balances"][0],
            schedule,
            case["coupon_rates"],
            counter,
            settings,
            payment_convention=BDC.Unadjusted,
            redemption=case["redemption"],
            issue_date=dates[0],
            ex_coupon_period=Period(case["ex_days"], "Days") if case["ex_days"] else None,
            ex_coupon_calendar=calendar if case["ex_days"] else None,
        )
    curve = FlatForward(date(case["reference"]), case["discount_rate"], DayCounter.actual365_fixed())
    return bond, curve, date(case["settlement"])


@pytest.mark.parametrize("row", ROWS, ids=[f"{r['id']}-{r['price_kind']}-{r['price']}" for r in ROWS])
def test_exact_independent_fixed_and_zero_rows(row: dict[str, str]) -> None:
    """Pin ATM2e-13, accrued2e-12 and exact current notional separately."""
    bond, curve, settlement = build(CASES[row["id"]])
    kind = BondPriceType.Clean if row["price_kind"] == "clean" else BondPriceType.Dirty
    actual = bond.atm_rate_from_price(curve, float(row["price"]), price_type=kind, settlement=settlement)
    assert math.isfinite(actual)
    for key in ["native_atm", "independent_atm_100digits"]:
        expected = float(row[key])
        assert math.isfinite(expected) and abs(actual - expected) <= 2e-13
    assert bond.notional(settlement) == float(row["current_notional"])
    accrued = bond.accrued_amount(settlement)
    assert math.isfinite(accrued) and abs(accrued - float(row["accrued_per100"])) <= 2e-12
    assert not bond.is_calculated()


@pytest.mark.parametrize("row", ERRORS, ids=[f"{r['id']}-{r['price_kind']}-{r['price']}" for r in ERRORS])
def test_independent_maturity_and_zero_bps_errors(row: dict[str, str]) -> None:
    """Native domain errors remain required, not tolerated numerical differences."""
    bond, curve, settlement = build(CASES[row["id"]])
    kind = BondPriceType.Clean if row["price_kind"] == "clean" else BondPriceType.Dirty
    with pytest.raises(ItofinError):
        bond.atm_rate_from_price(curve, float(row["price"]), price_type=kind, settlement=settlement)
