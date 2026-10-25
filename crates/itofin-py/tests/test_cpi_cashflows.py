"""Explicit-base plain CPI cash flows against independently generated QuantLib data."""
import gc
import json
from pathlib import Path

import pytest

from itofin import ItofinError, Settings
from itofin.cashflows import CPICoupon, CPICouponPricer, CPILeg, npv, npv_at_z_spread, z_spread
from itofin.indexes import CpiInterpolationType, ZeroInflationIndex
from itofin.termstructures import FlatForward, InterpolatedZeroInflationCurve
from itofin.time import BusinessDayConvention, Calendar, Compounding, Date, DayCounter, Frequency, Period, Schedule

DATA = Path(__file__).resolve().parents[3] / "sdk/go/testdata"
INPUTS = json.loads((DATA / "cpi-oracle-inputs.json").read_text())["inputs"]
COUPONS = json.loads((DATA / "cpi-oracle-coupons.json").read_text())["coupons"]
LEGS = json.loads((DATA / "cpi-oracle-legs.json").read_text())["legs"]


def date(iso):
    year, month, day = map(int, iso.split("-"))
    return Date(day, month, year)


def setup(rate=0.025):
    settings = Settings()
    settings.set_evaluation_date(date(INPUTS["evaluation_date"]))
    index = ZeroInflationIndex.uk_rpi(settings)
    for row in INPUTS["historical_fixings"]:
        index.add_fixing(date(row["date"]), row["value"])
    dc = DayCounter.actual365_fixed()
    curve = InterpolatedZeroInflationCurve(date(INPUTS["evaluation_date"]), [date(x) for x in INPUTS["inflation_curve_dates"]], [rate, rate], Frequency.Monthly, dc)
    index.link_to(curve)
    discount = FlatForward(date(INPUTS["evaluation_date"]), 0.04, dc)
    return settings, index, dc, discount


@pytest.mark.parametrize("kind", [CpiInterpolationType.Flat, CpiInterpolationType.Linear])
def test_coupon_historical_accrued_and_plain_pricer(kind):
    settings, index, dc, discount = setup()
    label = "flat" if kind == CpiInterpolationType.Flat else "linear"
    row = next(x["values"] for x in COUPONS if x["name"] == label + "_historical_excoupon")
    coupon = CPICoupon(100.0, Date(15, 7, 2025), 100000.0, Date(15, 1, 2025), Date(15, 7, 2025), index, Period(3, "Months"), kind, dc, 0.02, ex_coupon_date=Date(10, 7, 2025))
    with pytest.raises(ItofinError):
        coupon.rate()
    pricer = CPICouponPricer(discount)
    coupon.set_pricer(pricer)
    assert coupon.rate() == pytest.approx(row["rate"], abs=1e-13, rel=0)
    assert coupon.amount() == pytest.approx(row["amount"], abs=1e-9, rel=0)
    assert coupon.index_fixing() == pytest.approx(row["index_fixing"], abs=1e-12, rel=0)
    assert coupon.index_ratio(Date(15, 7, 2025)) == pytest.approx(row["index_fixing"] / 100, abs=1e-13, rel=0)
    assert coupon.accrual_period() == pytest.approx(row["accrual_period"], abs=1e-14, rel=0)
    for accrued in row["accrued"]:
        assert coupon.accrued_amount(date(accrued["date"])) == pytest.approx(accrued["amount"], abs=1e-9, rel=0)
    assert pricer.swaplet_rate(coupon) == coupon.rate()
    expected = coupon.rate() * coupon.accrual_period() * discount.discount_date(date(row["payment"]), False)
    del discount
    gc.collect()
    assert pricer.swaplet_price(coupon) == pytest.approx(expected, abs=1e-12, rel=0)
    level = 100.8 if kind == CpiInterpolationType.Flat else 100.8 + 0.2 * 9 / 31
    assert pricer.accrued_rate(coupon, Date(10, 7, 2025)) == pytest.approx(0.02 * level / 100, abs=1e-13, rel=0)
    del settings, index, dc, pricer
    gc.collect()
    assert coupon.amount() == pytest.approx(row["amount"], abs=1e-9, rel=0)


@pytest.mark.parametrize("row", LEGS)
def test_leg_inspection_npv_lifetime_and_settings(row):
    settings, index, dc, discount = setup()
    kind = CpiInterpolationType.Flat if row["interpolation"] == "flat" else CpiInterpolationType.Linear
    cal = Calendar.null_calendar()
    schedule = Schedule(Date(15, 1, 2025), Date(15, 1, 2028), Frequency.Semiannual, cal, BusinessDayConvention.Unadjusted)
    builder = CPILeg(schedule, index, 100.0, Period(3, "Months"), [100000.0], [0.02], dc, cal, BusinessDayConvention.Unadjusted, kind, row["growth_only"])
    leg = builder.build()
    assert len(leg) == len(row["cashflows"])
    for i, expected in enumerate(row["cashflows"]):
        assert leg[i].date() == date(expected["date"])
        assert leg[i].amount() == pytest.approx(expected["amount"], abs=1e-8, rel=0)
    assert npv(leg, discount, settings, False, date(INPUTS["evaluation_date"]), date(INPUTS["evaluation_date"])) == pytest.approx(row["npv"], abs=1e-8, rel=0)
    other = Settings()
    other.set_evaluation_date(date(INPUTS["evaluation_date"]))
    with pytest.raises(ValueError, match="settings must match"):
        npv(leg, discount, other)
    with pytest.raises(ValueError, match="settings must match"):
        npv_at_z_spread(leg, discount, 0.0, Compounding.Continuous, Frequency.Annual, other)
    with pytest.raises(ValueError, match="settings must match"):
        z_spread(leg, row["npv"], discount, Compounding.Continuous, Frequency.Annual, other)
    principal = leg[-1]
    del leg, builder, index, schedule, cal
    gc.collect()
    assert principal.amount() == pytest.approx(row["cashflows"][-1]["amount"], abs=1e-8, rel=0)


@pytest.mark.parametrize("base", [0.0, 1e-20, float("nan"), float("inf")])
def test_invalid_base_rejected(base):
    _, index, dc, _ = setup()
    with pytest.raises((ItofinError, ValueError)):
        CPICoupon(base, Date(15, 7, 2025), 100.0, Date(15, 1, 2025), Date(15, 7, 2025), index, Period(3, "Months"), CpiInterpolationType.Flat, dc, 0.02)


@pytest.mark.parametrize("kind,label", [(CpiInterpolationType.Flat, "flat"), (CpiInterpolationType.Linear, "linear")])
def test_forecast_coupon_relinks_live_inflation_curve(kind, label):
    _, index, dc, _ = setup()
    coupon = CPICoupon(100.0, Date(15, 7, 2026), 100000.0, Date(15, 1, 2026), Date(15, 7, 2026), index, Period(3, "Months"), kind, dc, 0.02)
    coupon.set_pricer(CPICouponPricer())
    for rate, suffix in [(0.025, "_forecast"), (0.03, "_forecast_relinked_3pct")]:
        curve = InterpolatedZeroInflationCurve(date(INPUTS["evaluation_date"]), [date(x) for x in INPUTS["inflation_curve_dates"]], [rate, rate], Frequency.Monthly, dc)
        index.link_to(curve)
        expected = next(x["values"] for x in COUPONS if x["name"] == label + suffix)
        assert coupon.amount() == pytest.approx(expected["amount"], abs=1e-9, rel=0)
        assert coupon.rate() == pytest.approx(expected["rate"], abs=1e-13, rel=0)
        assert coupon.index_fixing() == pytest.approx(expected["index_fixing"], abs=1e-11, rel=0)
    with pytest.raises(ItofinError):
        CPICouponPricer().swaplet_price(coupon)
