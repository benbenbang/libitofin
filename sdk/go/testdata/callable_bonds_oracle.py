"""Independent QuantLib 1.43 callable foundation and contractual analytics fixtures."""

import json

import QuantLib as q


results = {"quantlib_version": q.__version__, "cases": {}}
for kind in ("fixed", "zero", "ex_coupon"):
    today = q.Date(15, 1, 2024)
    q.Settings.instance().evaluationDate = today
    q.Settings.instance().includeReferenceDateEvents = False
    q.Settings.instance().includeTodaysCashFlows = None
    dc = q.Actual365Fixed()
    calendar = q.NullCalendar()
    maturity = q.Date(15, 1, 2027)
    schedule = q.Schedule(today, maturity, q.Period(q.Semiannual), calendar, q.Unadjusted, q.Unadjusted, q.DateGeneration.Backward, False)
    calls = q.CallabilitySchedule([
        q.Callability(q.BondPrice(100.0, q.BondPrice.Clean), q.Callability.Call, q.Date(15, 1, 2025)),
        q.Callability(q.BondPrice(95.0, q.BondPrice.Dirty), q.Callability.Put, q.Date(15, 1, 2025)),
        q.Callability(q.BondPrice(0.0, q.BondPrice.Dirty), q.Callability.Call, today),
    ])
    if kind == "zero":
        bond = q.CallableZeroCouponBond(2, 1000.0, calendar, maturity, dc, q.Following, 100.0, today, calls)
    else:
        ex = q.Period(7, q.Days) if kind == "ex_coupon" else q.Period()
        bond = q.CallableFixedRateBond(2, 1000.0, schedule, [0.05], dc, q.Following, 100.0, today, calls, ex, calendar, q.Unadjusted, False)
    if kind == "ex_coupon":
        q.Settings.instance().evaluationDate = q.Date(9, 7, 2024)
    curve = q.FlatForward(today, 0.04, dc)
    settlement = bond.settlementDate()
    clean_yield = q.BondFunctions.cleanPrice(bond, 0.047, dc, q.Compounded, q.Semiannual, settlement)
    clean_spread = q.BondFunctions.cleanPrice(bond, curve, 0.006, q.Compounded, q.Semiannual, settlement)
    results["cases"][kind] = {
        "settlement": settlement.serialNumber(), "maturity": bond.maturityDate().serialNumber(),
        "issue": bond.issueDate().serialNumber(), "notional": bond.notional(),
        "accrued": bond.accruedAmount(), "cashflows": [[f.date().serialNumber(), f.amount()] for f in bond.cashflows()],
        "callability": [[c.price().amount(), 0 if c.price().type() == q.BondPrice.Clean else 1, int(c.type()), c.date().serialNumber()] for c in bond.callability()],
        "clean_curve": q.BondFunctions.cleanPrice(bond, curve, settlement),
        "dirty_curve": q.BondFunctions.dirtyPrice(bond, curve, settlement),
        "clean_yield": clean_yield, "dirty_yield": clean_yield + bond.accruedAmount(),
        "clean_spread": clean_spread,
        "dirty_spread": q.BondFunctions.dirtyPrice(bond, curve, 0.006, q.Compounded, q.Semiannual, settlement),
        "yield": q.BondFunctions.bondYield(bond, q.BondPrice(clean_yield, q.BondPrice.Clean), dc, q.Compounded, q.Semiannual, settlement, 1e-12, 1000, 0.05),
        "spread": q.BondFunctions.zSpread(bond, q.BondPrice(clean_spread, q.BondPrice.Clean), curve, q.Compounded, q.Semiannual, settlement, 1e-12, 1000, 0.0),
    }
print(json.dumps(results, indent=2, sort_keys=True))
