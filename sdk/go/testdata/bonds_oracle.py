"""QuantLib 1.43 fixtures for fixed/zero bond bindings."""

import json

import QuantLib as q


def snapshot(bond):
    return {
        "npv": bond.NPV(),
        "settlement_value": bond.settlementValue(),
        "clean": bond.cleanPrice(),
        "dirty": bond.dirtyPrice(),
        "accrued": bond.accruedAmount(),
        "settlement": bond.settlementDate().serialNumber(),
        "notional": bond.notional(),
        "expired": bond.isExpired(),
        "cashflows": [[f.date().serialNumber(), f.amount()] for f in bond.cashflows()],
    }


results = {"quantlib_version": q.__version__, "cases": {}}
for kind in ("fixed", "zero", "ex_coupon"):
    today = q.Date(15, 1, 2024)
    q.Settings.instance().evaluationDate = today
    q.Settings.instance().includeReferenceDateEvents = False
    q.Settings.instance().includeTodaysCashFlows = None
    dc = q.Actual365Fixed()
    calendar = q.NullCalendar()
    quote = q.SimpleQuote(0.04)
    curve = q.FlatForward(today, q.QuoteHandle(quote), dc)
    schedule = q.Schedule(today, q.Date(15, 1, 2027), q.Period(q.Semiannual), calendar, q.Unadjusted, q.Unadjusted, q.DateGeneration.Backward, False)
    if kind == "zero":
        bond = q.ZeroCouponBond(2, calendar, 1000.0, q.Date(15, 1, 2027), q.Following, 100, today)
    elif kind == "ex_coupon":
        bond = q.FixedRateBond(2, 1000.0, schedule, [0.05], dc, q.Following, 100.0, today, calendar, q.Period(7, q.Days), calendar, q.Unadjusted, False)
        q.Settings.instance().evaluationDate = q.Date(9, 7, 2024)
    else:
        bond = q.FixedRateBond(2, 1000.0, schedule, [0.05], dc, q.Following, 100.0, today)
    bond.setPricingEngine(q.DiscountingBondEngine(q.YieldTermStructureHandle(curve)))
    case = {"initial": snapshot(bond)}
    quote.setValue(0.055)
    case["updated"] = snapshot(bond)
    q.Settings.instance().evaluationDate = q.Date(16, 1, 2027)
    case["expired"] = snapshot(bond)
    results["cases"][kind] = case
print(json.dumps(results, indent=2, sort_keys=True))
