"""Generate supported callable-bond event-order and ex-coupon references."""

import QuantLib as ql

from generate import emit


def callable_edges():
    start, maturity = ql.Date(7, 7, 2026), ql.Date(7, 7, 2031)
    dc = ql.Actual365Fixed()
    schedule = ql.Schedule(
        start,
        maturity,
        ql.Period(6, ql.Months),
        ql.NullCalendar(),
        ql.Unadjusted,
        ql.Unadjusted,
        ql.DateGeneration.Forward,
        False,
    )
    rows = []
    for name, reference, ex_coupon_days, events in [
        (
            "ex_coupon_close_call",
            start,
            7,
            [("call", "clean", 100, ql.Date(5, 7, 2028))],
        ),
        (
            "ex_coupon_settlement_clean_call",
            ql.Date(3, 7, 2028),
            7,
            [("call", "clean", 100, ql.Date(7, 8, 2028))],
        ),
        (
            "ex_coupon_settlement_dirty_call",
            ql.Date(3, 7, 2028),
            7,
            [("call", "dirty", 100, ql.Date(7, 8, 2028))],
        ),
        (
            "multiple_call_put",
            start,
            0,
            [
                ("call", "clean", 100, ql.Date(7, 7, 2028)),
                ("put", "clean", 101, ql.Date(7, 7, 2029)),
            ],
        ),
        ("maturity_call", start, 0, [("call", "clean", 100, maturity)]),
    ]:
        ql.Settings.instance().evaluationDate = reference
        curve = ql.YieldTermStructureHandle(ql.FlatForward(reference, 0.03, dc))
        model = ql.HullWhite(curve, 0.1, 0.01)
        callability = ql.CallabilitySchedule()
        for kind, quote_type, price, date in events:
            callability.append(
                ql.Callability(
                    ql.BondPrice(
                        price,
                        ql.BondPrice.Clean
                        if quote_type == "clean"
                        else ql.BondPrice.Dirty,
                    ),
                    ql.Callability.Call if kind == "call" else ql.Callability.Put,
                    date,
                )
            )
        bond = ql.CallableFixedRateBond(
            2,
            1000,
            schedule,
            [0.05],
            dc,
            ql.Unadjusted,
            100,
            start,
            callability,
            ql.Period(ex_coupon_days, ql.Days),
            ql.NullCalendar(),
            ql.Unadjusted,
            False,
        )
        bond.setPricingEngine(ql.TreeCallableFixedRateBondEngine(model, 100))
        rows.append(
            dict(
                name=name,
                reference=reference.ISO(),
                ex_coupon_days=ex_coupon_days,
                events=[
                    dict(kind=k, quote_type=t, price=p, date=d.ISO())
                    for k, t, p, d in events
                ],
                npv=bond.NPV(),
                settlement_value=bond.settlementValue(),
                dirty=bond.dirtyPrice(),
                clean=bond.cleanPrice(),
                accrued=bond.accruedAmount(),
                settlement=bond.settlementDate().ISO(),
            )
        )
    emit(
        "callable-edge-cases.json",
        dict(
            issue=start.ISO(),
            maturity=maturity.ISO(),
            day_counter="Actual365Fixed",
            coupon=0.05,
            tenor_months=6,
            face=1000,
            redemption_per_100=100,
            risk_free=0.03,
            hull_white_a=0.1,
            hull_white_sigma=0.01,
            tree_steps=100,
            settlement_days=2,
            calendar="NullCalendar",
            convention="Unadjusted",
            ex_coupon_convention="Unadjusted",
        ),
        rows,
    )


if __name__ == "__main__":
    callable_edges()
