"""Emit the adopted QuantLib 1.43 callable clean-price sensitivity profile."""
import csv
import io
import sys

import QuantLib as ql

assert ql.__version__ == "1.43"
reference, issue, maturity = ql.Date(12, 10, 2026), ql.Date(7, 7, 2026), ql.Date(7, 7, 2031)
ql.Settings.instance().evaluationDate = reference
dc, cal = ql.Actual365Fixed(), ql.NullCalendar()
curve = ql.YieldTermStructureHandle(ql.FlatForward(reference, 0.03, dc))
engine = ql.TreeCallableFixedRateBondEngine(ql.HullWhite(curve, 0.1, 0.01), 100)
output = io.StringIO()
writer = csv.writer(output, delimiter="\t", lineterminator="\n")
writer.writerow(["kind", "right", "compounding", "bump", "accrued", "center", "up", "down", "duration", "convexity"])
for kind in ["fixed", "zero"]:
    for right in ["none", "call", "put"]:
        calls = [] if right == "none" else [ql.Callability(ql.BondPrice(100.0, ql.BondPrice.Clean), ql.Callability.Call if right == "call" else ql.Callability.Put, ql.Date(7, 7, 2028))]
        if kind == "fixed":
            schedule = ql.Schedule(issue, maturity, ql.Period(ql.Semiannual), cal, ql.Unadjusted, ql.Unadjusted, ql.DateGeneration.Backward, False)
            bond = ql.CallableFixedRateBond(0, 1000.0, schedule, [0.05], dc, ql.Unadjusted, 100.0, issue, calls)
        else:
            bond = ql.CallableZeroCouponBond(0, 1000.0, cal, maturity, dc, ql.Unadjusted, 100.0, issue, calls)
        bond.setPricingEngine(engine)
        for label, comp, freq in [("continuous", ql.Continuous, ql.Annual), ("compounded", ql.Compounded, ql.Semiannual), ("simple", ql.Simple, ql.Annual)]:
            for bump in [2e-4, 0.001]:
                spread = 0.0075
                prices = [bond.cleanPriceOAS(s, curve, dc, comp, freq) for s in [spread, spread+bump, spread-bump]]
                values = [bump, bond.accruedAmount(), *prices, bond.effectiveDuration(spread, curve, dc, comp, freq, bump), bond.effectiveConvexity(spread, curve, dc, comp, freq, bump)]
                writer.writerow([kind, right, label, *[format(value, ".17g") for value in values]])
sys.stdout.write(output.getvalue())
