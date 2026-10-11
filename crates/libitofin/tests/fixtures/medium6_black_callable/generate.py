"""Regenerate Black callable fixtures with QuantLib 1.43 and explicit PV units."""

import math
from pathlib import Path

import QuantLib as ql


def generate_rows():
    today = ql.Date(15, 1, 2026)
    ql.Settings.instance().evaluationDate = today
    maturity = ql.Date(15, 1, 2031)
    exercise = ql.Date(15, 1, 2028)
    day_counter = ql.Actual365Fixed()
    curve = ql.YieldTermStructureHandle(
        ql.FlatForward(today, 0.04, day_counter, ql.Continuous, ql.Annual)
    )
    rows = []
    for zero in (False, True):
        for direction in (ql.Callability.Call, ql.Callability.Put):
            for lag in (0, 3, 15):
                for volatility in (0.0, 0.1, 0.35):
                    rights = ql.CallabilitySchedule()
                    rights.append(
                        ql.Callability(
                            ql.BondPrice(100.0, ql.BondPrice.Clean), direction, exercise
                        )
                    )
                    if zero:
                        bond = ql.CallableZeroCouponBond(
                            lag, 1000.0, ql.NullCalendar(), maturity, day_counter,
                            ql.Unadjusted, 100.0, today, rights,
                        )
                        frequency = ql.Annual
                    else:
                        schedule = ql.Schedule(
                            today, maturity, ql.Period(ql.Semiannual), ql.NullCalendar(),
                            ql.Unadjusted, ql.Unadjusted, ql.DateGeneration.Backward, False,
                        )
                        bond = ql.CallableFixedRateBond(
                            lag, 1000.0, schedule, [0.055], day_counter,
                            ql.Unadjusted, 100.0, today, rights,
                        )
                        frequency = ql.Semiannual
                    bond.setPricingEngine(
                        ql.BlackCallableFixedRateBondEngine(
                            ql.QuoteHandle(ql.SimpleQuote(volatility)), curve
                        )
                    )
                    settlement = bond.settlementDate()
                    flows = bond.cashflows()
                    value = ql.CashFlows.npv(flows, curve, False, settlement, settlement)
                    npv = ql.CashFlows.npv(flows, curve, False, today, today)
                    settlement_discount = curve.discount(settlement)
                    exercise_discount = curve.discount(exercise)
                    income = sum(
                        flow.amount() * curve.discount(flow.date())
                        for flow in list(flows)[:-1]
                        if settlement < flow.date() <= exercise
                    ) / settlement_discount
                    forward_npv = ql.CashFlows.npv(
                        flows, curve, False, exercise, exercise
                    )
                    forward_yield = ql.CashFlows.yieldRate(
                        flows, forward_npv, day_counter, ql.Compounded, frequency,
                        False, exercise, exercise,
                    )
                    duration = ql.CashFlows.duration(
                        flows, forward_yield, day_counter, ql.Compounded, frequency,
                        ql.Duration.Modified, False, exercise, exercise,
                    )
                    std_dev = volatility * duration * forward_yield * math.sqrt(
                        day_counter.yearFraction(today, exercise)
                    )
                    strike = (100.0 + bond.accruedAmount(exercise)) * 10.0
                    native_forward = (value - income) / exercise_discount
                    forward = (value - income) * settlement_discount / exercise_discount
                    direct_forward = sum(
                        flow.amount() * curve.discount(flow.date())
                        for flow in flows if flow.date() > exercise
                    ) / exercise_discount
                    assert abs(forward - direct_forward) < 1.0e-10
                    put = direction == ql.Callability.Put
                    sign = 1.0 if put else -1.0
                    option_type = ql.Option.Put if put else ql.Option.Call
                    option = ql.blackFormula(option_type, strike, forward, std_dev)
                    native_option = ql.blackFormula(
                        option_type, strike, native_forward, std_dev
                    )
                    corrected_npv = npv + sign * option * exercise_discount
                    corrected_settlement = (
                        value + sign * option * exercise_discount / settlement_discount
                    )
                    native_settlement = (
                        value + sign * native_option * exercise_discount / settlement_discount
                    )
                    assert abs(native_settlement - bond.settlementValue()) < 1.0e-9
                    rows.append((
                        zero, put, lag, volatility, corrected_npv, corrected_settlement,
                    ))
    return rows


def main():
    if ql.__version__ != "1.43":
        raise RuntimeError("This fixture is pinned to QuantLib 1.43")
    rows = generate_rows()
    target = (
        Path(__file__).resolve().parent.parents[2]
        / "src/pricingengines/bond/black_callable_oracle.csv"
    )
    with target.open("w") as output:
        output.write("zero,put,lag,vol,npv,settlement\n")
        for row in rows:
            output.write(",".join(str(value).lower() for value in row) + "\n")
    print(f"Wrote {len(rows)} independently normalized QuantLib rows to {target}")


if __name__ == "__main__":
    main()
