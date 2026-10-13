"""Independent QuantLib 1.43 cashflow/yield/duration and Black-formula oracle."""

import argparse
import json
import math
from pathlib import Path

import QuantLib as q


def serial(day, month, year):
    return q.Date(day, month, year).serialNumber()


def fixture(name, zero=False, put=False, lag=0, vol=0.1, **overrides):
    row = dict(
        name=name,
        kind="zero" if zero else "fixed",
        put=put,
        settlement_days=lag,
        volatility=vol,
        reference=serial(15, 1, 2026),
        evaluation=serial(15, 1, 2026),
        issue=serial(15, 1, 2026),
        start=serial(15, 1, 2026),
        exercise=serial(15, 1, 2028),
        maturity=serial(15, 1, 2031),
        face=1000.0,
        redemption=100.0,
        strike=100.0,
        price_type=0,
        frequency=1,
        schedule_rule=0,
        coupons=[0.055],
        day_counter=1,
        rate=0.04,
        curve_dates=[],
        curve_rates=[],
        ex_length=0,
        ex_unit=0,
        ex_eom=False,
        payment_convention=2,
        calendar="null",
    )
    row.update(overrides)
    return row


def product(row):
    dc = q.Actual365Fixed() if row["day_counter"] == 1 else q.Actual360()
    calendar = q.NullCalendar() if row["calendar"] == "null" else q.TARGET()
    calls = q.CallabilitySchedule()
    calls.append(
        q.Callability(
            q.BondPrice(
                row["strike"],
                q.BondPrice.Clean if row["price_type"] == 0 else q.BondPrice.Dirty,
            ),
            q.Callability.Put if row["put"] else q.Callability.Call,
            q.Date(row["exercise"]),
        )
    )
    args = (
        row["settlement_days"],
        row["face"],
    )
    common = (
        dc,
        row["payment_convention"],
        row["redemption"],
        q.Date(row["issue"]),
        calls,
    )
    if row["kind"] == "zero":
        return (
            q.CallableZeroCouponBond(*args, calendar, q.Date(row["maturity"]), *common),
            dc,
            q.Annual,
        )
    schedule = q.Schedule(
        q.Date(row["start"]),
        q.Date(row["maturity"]),
        q.Period(q.Annual if row["frequency"] == 0 else q.Semiannual),
        calendar,
        q.Unadjusted,
        q.Unadjusted,
        q.DateGeneration.Zero
        if row["schedule_rule"] == 2
        else q.DateGeneration.Backward,
        False,
    )
    bond = q.CallableFixedRateBond(
        *args,
        schedule,
        row["coupons"],
        *common,
        q.Period(row["ex_length"], q.Days if row["ex_unit"] == 0 else q.Months),
        q.NullCalendar(),
        q.Unadjusted,
        row["ex_eom"],
    )
    frequency = (
        q.Annual if row["frequency"] == 0 or row["schedule_rule"] == 2 else q.Semiannual
    )
    return bond, dc, frequency


def snapshot(row, bond, dc, frequency, curve, volatility):
    reference = curve.referenceDate()
    settlement = bond.settlementDate()
    exercise = q.Date(row["exercise"])
    flows = bond.cashflows()
    ds, de = curve.discount(settlement), curve.discount(exercise)

    def owned(flow, date):
        coupon = q.as_coupon(flow)
        return (
            coupon is None
            or coupon.exCouponDate() == q.Date()
            or coupon.exCouponDate() > date
        )

    value = q.CashFlows.npv(flows, curve, False, settlement, settlement)
    npv = q.CashFlows.npv(flows, curve, False, reference, reference)
    income_reference = sum(
        flow.amount() * curve.discount(flow.date())
        for flow in list(flows)[:-1]
        if settlement < flow.date() <= exercise and owned(flow, settlement)
    )
    forward = (value * ds - income_reference) / de
    direct = sum(
        flow.amount() * curve.discount(flow.date()) / de
        for flow in flows
        if flow.date() > exercise and owned(flow, settlement)
    )
    assert abs(forward - direct) < max(1e-10, abs(forward) * 2e-15)
    exercise_pv = q.CashFlows.npv(flows, curve, False, exercise, exercise)
    yield_bond, _, _ = product(dict(row, face=1000.0))
    yield_flows = yield_bond.cashflows()
    yield_pv = q.CashFlows.npv(yield_flows, curve, False, exercise, exercise)
    zero_yield_pv = q.CashFlows.npv(
        yield_flows, 0.0, dc, q.Compounded, frequency, False, exercise, exercise
    )
    rate = (
        0.0
        if yield_pv == zero_yield_pv
        else q.CashFlows.yieldRate(
            yield_flows,
            yield_pv,
            dc,
            q.Compounded,
            frequency,
            False,
            exercise,
            exercise,
        )
    )
    duration = q.CashFlows.duration(
        yield_flows,
        rate,
        dc,
        q.Compounded,
        frequency,
        q.Duration.Modified,
        False,
        exercise,
        exercise,
    )
    indenture_accrued = 0.0
    for flow in flows:
        if flow.date() > exercise:
            coupon = q.as_coupon(flow)
            if coupon is not None:
                indenture_accrued = coupon.accruedAmount(exercise)
                if not owned(flow, exercise):
                    indenture_accrued += flow.amount()
            break
    strike = row["strike"] * row["face"] / 100
    if row["price_type"] == 0:
        strike += indenture_accrued
    time = q.Actual365Fixed().yearFraction(q.Date(row["evaluation"]), exercise)
    stddev = volatility * duration * rate * math.sqrt(time)
    option = q.blackFormula(
        q.Option.Put if row["put"] else q.Option.Call, strike, forward, stddev
    )
    sign = 1 if row["put"] else -1
    result = npv + sign * option * de
    settlement_value = value + sign * option * de / ds
    dirty = settlement_value / bond.notional(settlement) * 100
    accrued = bond.accruedAmount(settlement)
    return dict(
        npv=result,
        settlement_value=settlement_value,
        clean_price=dirty - accrued,
        dirty_price=dirty,
        accrued=accrued,
        settlement=settlement.serialNumber(),
        settlement_discount=ds,
        exercise_discount=de,
        straight_reference=npv,
        straight_settlement=value,
        interim_income_reference=income_reference,
        forward=forward,
        direct_settlement_owned_forward=direct,
        exercise_owned_forward_pv=exercise_pv,
        yield_rate=rate,
        duration=duration,
        indenture_accrued=indenture_accrued,
        strike_currency=strike,
        exercise_time=time,
        standard_deviation=stddev,
        option=option,
    )


def main():
    assert q.__version__ == "1.43"
    parser = argparse.ArgumentParser()
    parser.add_argument("--raw-output", type=Path)
    args = parser.parse_args()
    rows = [
        fixture(f"{kind}_{direction}_lag{lag}_vol{vol}", zero, put, lag, vol)
        for kind, zero in (("fixed", False), ("zero", True))
        for direction, put in (("call", False), ("put", True))
        for lag in (0, 3, 15)
        for vol in (0.0, 0.1, 0.35)
    ]
    rows += [
        fixture("fixed_dirty_exercise", price_type=1, exercise=serial(12, 10, 2028)),
        fixture("fixed_clean_off_coupon", exercise=serial(12, 10, 2028), lag=2),
        fixture(
            "fixed_reference_before_evaluation", evaluation=serial(23, 1, 2026), lag=2
        ),
        fixture("fixed_a360", day_counter=0, lag=10),
        fixture("fixed_once", schedule_rule=2, lag=2),
        fixture(
            "fixed_nonflat",
            lag=10,
            curve_dates=[serial(15, 1, 2026), serial(15, 1, 2028), serial(15, 1, 2032)],
            curve_rates=[0.035, 0.041, 0.047],
        ),
        fixture(
            "fixed_ex_coupon_settlement",
            reference=serial(10, 1, 2026),
            evaluation=serial(10, 1, 2026),
            start=serial(15, 1, 2025),
            issue=serial(15, 1, 2025),
            ex_length=10,
            vol=0.2,
        ),
        fixture(
            "fixed_ex_coupon_exercise",
            exercise=serial(10, 1, 2028),
            ex_length=10,
            lag=2,
            vol=0.2,
        ),
        fixture(
            "fixed_ex_coupon_dirty_exercise",
            exercise=serial(10, 1, 2028),
            ex_length=10,
            price_type=1,
            put=True,
            vol=0.2,
        ),
        fixture(
            "zero_reference_before_evaluation",
            zero=True,
            evaluation=serial(23, 1, 2026),
            lag=2,
            strike=85,
        ),
        fixture(
            "zero_exact_zero_yield",
            zero=True,
            rate=0,
            redemption=107.5,
            strike=97.25,
            lag=17,
            exercise=serial(2, 2, 2026),
            vol=0.23,
        ),
        fixture(
            "zero_exact_zero_yield_put",
            zero=True,
            rate=0,
            redemption=107.5,
            strike=97.25,
            put=True,
            vol=0.23,
        ),
        fixture(
            "zero_tiny_positive_yield", zero=True, rate=1e-8, strike=99.9999, vol=0.23
        ),
        fixture("zero_negative_yield_intrinsic", zero=True, rate=-0.01, vol=0),
        fixture("zero_zero_strike", zero=True, strike=0, put=True, lag=2),
        fixture("zero_small_face", zero=True, face=1e-25, vol=0.35, lag=3, put=True),
        fixture("zero_large_face", zero=True, face=1e12, vol=0.35, lag=3, put=True),
    ]
    unidentifiable = [
        f"zero_{direction}_lag{lag}_vol0.1"
        for direction in ("call", "put")
        for lag in (0, 3, 15)
    ]
    rows += [
        fixture(
            f"zero_{direction}_lag{lag}_vol0.1_strike85", True, put, lag, 0.1, strike=85
        )
        for direction, put in (("call", False), ("put", True))
        for lag in (0, 3, 15)
    ]
    for row in rows:
        if row["name"] in unidentifiable:
            row["iv_known_vol_unidentifiable"] = True
            row["iv_qualification"] = (
                "Independent per100 vega2.90e-11, one-ULP vol uncertainty4.90e-4: retain prices, bracket and reprice, not unique vol2e-8. Exactly these six cases were independently approved."
            )
    raw = dict(quantlib_version=q.__version__, cases=[])
    for row in rows:
        settings = q.Settings.instance()
        settings.evaluationDate = q.Date(row["evaluation"])
        settings.includeReferenceDateEvents = False
        settings.includeTodaysCashFlows = None
        quote = q.SimpleQuote(row["rate"])
        curve = q.YieldTermStructureHandle(
            q.ZeroCurve(
                [q.Date(x) for x in row["curve_dates"]],
                row["curve_rates"],
                q.Actual365Fixed(),
            )
            if row["curve_dates"]
            else q.FlatForward(
                q.Date(row["reference"]), q.QuoteHandle(quote), q.Actual365Fixed()
            )
        )
        bond, dc, frequency = product(row)
        volatility = q.SimpleQuote(row["volatility"])
        bond.setPricingEngine(
            q.BlackCallableFixedRateBondEngine(q.QuoteHandle(volatility), curve)
        )
        try:
            native = dict(
                npv=bond.NPV(),
                settlement_value=bond.settlementValue(),
                clean_price=bond.cleanPrice(),
                dirty_price=bond.dirtyPrice(),
            )
        except RuntimeError as error:
            native = dict(error=str(error))
        raw["cases"].append(dict(name=row["name"], native=native))
        row["expected"] = snapshot(row, bond, dc, frequency, curve, row["volatility"])
        if row["name"] != "zero_negative_yield_intrinsic":
            row["lower_endpoint"] = snapshot(row, bond, dc, frequency, curve, 0.0)
            row["upper_endpoint"] = snapshot(row, bond, dc, frequency, curve, 1.0)
        if row["name"] == "fixed_call_lag3_vol0.1":
            row["updated_volatility"] = snapshot(row, bond, dc, frequency, curve, 0.42)
            quote.setValue(0.051)
            row["updated_rate"] = snapshot(row, bond, dc, frequency, curve, 0.42)
    if args.raw_output:
        args.raw_output.write_text(json.dumps(raw, indent=2, sort_keys=True) + "\n")
    print(
        json.dumps(
            dict(
                quantlib_version=q.__version__,
                cases=rows,
                iv_known_vol_unidentifiable_cases=unidentifiable,
            ),
            indent=2,
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
