"""Live base-date CPI paths against an independent QuantLib 1.43 matrix."""
import csv
import gc
import json
from pathlib import Path

import pytest

from itofin import ItofinError, Settings
from itofin.cashflows import CPICoupon, CPICouponPricer, CPILeg, npv
from itofin.indexes import CpiInterpolationType, ZeroInflationIndex
from itofin.termstructures import FlatForward, InterpolatedZeroInflationCurve
from itofin.time import BusinessDayConvention, Calendar, Date, DateGeneration, DayCounter, Frequency, Period, Schedule

DATA = Path(__file__).resolve().parents[3] / "sdk/go/testdata"
META = json.loads((DATA / "cpi-base-date-meta.json").read_text())
with (DATA / "cpi-base-date-coupons.tsv").open() as handle:
    ROWS = list(csv.DictReader(handle, delimiter="\t"))
with (DATA / "cpi-base-date-fixings.tsv").open() as handle:
    FIXINGS = list(csv.DictReader(handle, delimiter="\t"))
CASES = [row for row in ROWS if row["stage"] == "original"]
assert len(ROWS) == 108 and len(CASES) == 36 and len(FIXINGS) == 43


def date(text):
    year, month, day = map(int, text.split("-"))
    return Date(day, month, year)


def market(row, modified=False, missing_base=False):
    settings = Settings()
    settings.set_evaluation_date(date(META["inputs"]["evaluation_date"]))
    index = ZeroInflationIndex.uk_rpi(settings)
    for fixing in FIXINGS:
        is_base = fixing["date"] == row["base_overwrite_date"]
        if missing_base and is_base:
            continue
        value = float(row["base_overwrite_value"]) if modified and is_base else float(fixing["value"])
        index.add_fixing(date(fixing["date"]), value)
    dc = DayCounter.actual365_fixed()
    return settings, index, dc


def link(index, dc, rate):
    curve = InterpolatedZeroInflationCurve(
        date(META["inputs"]["evaluation_date"]),
        [date(value) for value in META["inputs"]["inflation_curve_dates"]],
        [rate, rate], Frequency.Monthly, dc,
    )
    index.link_to(curve)
    return curve


def coupon(row, index, dc):
    kind = CpiInterpolationType.Flat if row["interpolation"] == "flat" else CpiInterpolationType.Linear
    result = CPICoupon.from_base_date(
        date(row["base_date"]), date(row["payment_date"]), float(row["nominal"]),
        date(row["start_date"]), date(row["end_date"]), index,
        Period(int(row["lag_months"]), "Months"), kind, dc, float(row["fixed_rate"]),
        ex_coupon_date=date(row["ex_coupon_date"]),
    )
    result.set_pricer(CPICouponPricer())
    return result


def check(c, row):
    for field, actual, tolerance in [
        ("amount", c.amount(), 1e-9), ("rate", c.rate(), 1e-13),
        ("base_fixing", c.base_fixing(), 1e-11), ("index_fixing", c.index_fixing(), 1e-11),
        ("index_ratio", c.index_ratio(date(row["end_date"])), 1e-13),
        ("accrual_period", c.accrual_period(), 1e-14),
    ]:
        assert actual == pytest.approx(float(row[field]), abs=tolerance, rel=0), field
    for position in range(8):
        actual = c.accrued_amount(date(row[f"accrued_{position}_date"]))
        assert actual == pytest.approx(float(row[f"accrued_{position}_value"]), abs=1e-9, rel=0)


@pytest.mark.parametrize("row", CASES, ids=lambda row: row["id"])
def test_base_date_coupon_native_matrix_relink_and_retention(row):
    settings, index, dc = market(row)
    curve = link(index, dc, META["inputs"]["inflation_rate"])
    c = coupon(row, index, dc)
    check(c, row)
    changed = next(value for value in ROWS if value["id"] == row["id"] and value["stage"] == "overwritten")
    settings, index, dc = market(row, modified=True)
    curve = link(index, dc, META["inputs"]["inflation_rate"])
    c = coupon(row, index, dc)
    check(c, changed)
    curve = link(index, dc, META["inputs"]["relinked_inflation_rate"])
    expected = next(value for value in ROWS if value["id"] == row["id"] and value["stage"] == "relinked")
    check(c, expected)
    del settings, index, dc, curve
    gc.collect()
    check(c, expected)


def test_base_date_coupon_missing_base_is_lazy_and_later_fixing_is_live():
    row = CASES[0]
    _, index, dc = market(row, missing_base=True)
    c = coupon(row, index, dc)
    with pytest.raises(ItofinError):
        c.base_fixing()
    with pytest.raises(ItofinError):
        c.amount()
    index.add_fixing(date(row["base_overwrite_date"]), 227.1)
    check(c, row)


def test_base_date_leg_default_zero_rate_lazy_principal_and_settings():
    settings = Settings()
    settings.set_evaluation_date(Date(1, 1, 2024))
    index = ZeroInflationIndex.uk_rpi(settings)
    dc = DayCounter.actual365_fixed()
    cal = Calendar.null_calendar()
    schedule = Schedule(Date(31, 1, 2024), Date(31, 7, 2024), Frequency.Semiannual, cal, BusinessDayConvention.Unadjusted)
    leg = CPILeg.from_base_date(schedule, index, Period(1, "Months"), [10000.0], [0.0], dc, cal, BusinessDayConvention.Unadjusted).build()
    assert leg[0].amount() == 0.0
    with pytest.raises(ItofinError):
        leg[-1].amount()
    settings.set_evaluation_date(Date(1, 1, 2025))
    index.add_fixing(Date(1, 12, 2023), 95.0)
    index.add_fixing(Date(1, 6, 2024), 150.0)
    assert leg[-1].amount() == pytest.approx(10000 * (150 / 95 - 1), abs=1e-10, rel=0)
    discount = FlatForward(Date(1, 1, 2024), 0.03, dc)
    expected = npv(leg, discount, settings, False, Date(1, 1, 2024), Date(1, 1, 2024))
    other = Settings()
    other.set_evaluation_date(Date(1, 1, 2025))
    with pytest.raises(ValueError, match="settings must match"):
        npv(leg, discount, other)
    del settings, index, schedule, cal
    gc.collect()
    assert expected > 0
    assert leg[-1].amount() > 0


@pytest.mark.parametrize("base,lag", [(Date(1, 1, 2024), Period(-1, "Months")), (Date(31, 12, 2199), Period(1, "Months"))])
def test_base_date_invalid_arithmetic_is_an_error(base, lag):
    _, index, dc = market(CASES[0])
    with pytest.raises((ItofinError, ValueError)):
        CPICoupon.from_base_date(base, Date(1, 7, 2024), 1.0, Date(1, 1, 2024), Date(1, 7, 2024), index, lag, CpiInterpolationType.Linear, dc, 0.02)


with (DATA / "cpi-base-date-leg-flows.tsv").open() as handle:
    LEG_FLOWS = list(csv.DictReader(handle, delimiter="\t"))
with (DATA / "cpi-base-date-leg-npv.tsv").open() as handle:
    LEG_NPVS = list(csv.DictReader(handle, delimiter="\t"))
LEG_CASES = [row for row in LEG_FLOWS if row["flow"] == "0"]
assert len(LEG_FLOWS) == 480 and len(LEG_NPVS) == 384 and len(LEG_CASES) == 96


def serial_date(serial):
    return Date(1, 1, 1901) + (int(serial) - 367)


@pytest.mark.parametrize("row", LEG_CASES, ids=lambda row: row["case"] + "-" + row["stage"])
def test_base_date_leg_native_irregular_principal_and_npv(row):
    start = Date(17, 2, 2024) if row["irregular"] == "1" else Date(31, 1, 2024)
    lag = int(row["lag_months"])
    base = Date(31, 1, 2024)
    if row["default_base"] == "1":
        month = start.month - lag
        base_year = start.year - (1 if month < 1 else 0)
        base_month = month + (12 if month < 1 else 0)
        base = Date(start.day, base_month, base_year)
    fixing_key = f"{base.year:04}-{base.month:02}-01"
    market_row = {"stage": "original" if row["stage"] == "0" else "overwritten",
                  "base_overwrite_date": fixing_key, "base_overwrite_value": "180.25"}
    settings, index, dc = market(market_row, modified=row["stage"] != "0")
    rate = 0.065 if row["stage"] == "2" else -0.0075
    curve = link(index, dc, rate)
    cal = Calendar.null_calendar()
    schedule = Schedule(start, Date(31, 1, 2026), Frequency.Semiannual, cal,
                        BusinessDayConvention.Unadjusted, DateGeneration.Backward)
    kind = CpiInterpolationType.Flat if row["interpolation"] == "flat" else CpiInterpolationType.Linear
    builder = CPILeg.from_base_date(
        schedule, index, Period(lag, "Months"), [8123.75, 7400.0, 7100.0, 6900.0, 6500.0],
        [-0.0125, 0.0, 0.0375], dc, Calendar.weekends_only(), BusinessDayConvention.Following,
        base_date=None if row["default_base"] == "1" else base, interpolation=kind,
        growth_only=row["growth_only"] == "1", ex_coupon_period=Period(7, "Days"),
        ex_coupon_calendar=cal,
    )
    leg = builder.build()
    flows = [value for value in LEG_FLOWS if value["case"] == row["case"] and value["stage"] == row["stage"]]
    assert len(leg) == len(flows) == 5
    for position, expected in enumerate(flows):
        assert leg[position].date() == serial_date(expected["date"])
        assert leg[position].amount() == pytest.approx(float(expected["amount"]), abs=1e-8, rel=0)
    discount = FlatForward(Date(1, 1, 2024), -0.015, dc)
    for expected in LEG_NPVS:
        if expected["case"] != row["case"] or expected["stage"] != row["stage"]:
            continue
        actual = npv(leg, discount, settings, expected["include"] == "1",
                     serial_date(expected["settlement"]), date(META["inputs"]["evaluation_date"]))
        assert actual == pytest.approx(float(expected["npv"]), abs=1e-8, rel=0)
    assert curve is not None
