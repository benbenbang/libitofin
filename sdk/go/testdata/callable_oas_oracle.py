"""Independent QuantLib 1.43 OAS with explicit settlement normalization."""

import argparse
import json
from pathlib import Path

import QuantLib as q


def generate():
    """Name conventions explicitly, never reinterpret native enum ordinals."""
    if q.__version__ != "1.43":
        raise RuntimeError("QuantLib 1.43 required")
    reference = q.Date(7, 7, 2026)
    maturity = q.Date(7, 7, 2031)
    q.Settings.instance().evaluationDate = reference
    dc = q.Actual365Fixed()
    curve = q.DiscountCurve([reference, q.Date(7, 7, 2027), q.Date(7, 7, 2029), q.Date(7, 7, 2036)], [1.0, 0.983, 0.915, 0.67], dc, q.NullCalendar())
    handle = q.YieldTermStructureHandle(curve)
    model = q.HullWhite(handle, 0.1, 0.01)
    engine = q.TreeCallableFixedRateBondEngine(model, 100)
    calls = q.CallabilitySchedule([
        q.Callability(q.BondPrice(100.0, q.BondPrice.Clean), q.Callability.Call, q.Date(7, 8, 2028)),
        q.Callability(q.BondPrice(97.0, q.BondPrice.Dirty), q.Callability.Put, q.Date(7, 1, 2029)),
    ])
    schedule = q.Schedule(reference, maturity, q.Period(q.Semiannual), q.NullCalendar(), q.Unadjusted, q.Unadjusted, q.DateGeneration.Backward, False)
    conventions = [("Continuous", q.Continuous), ("Compounded", q.Compounded), ("Simple", q.Simple), ("SimpleThenCompounded", q.SimpleThenCompounded), ("CompoundedThenSimple", q.CompoundedThenSimple)]
    rows = []
    for kind in ["fixed", "zero"]:
        for lag in [0, 2, 10]:
            if kind == "fixed":
                bond = q.CallableFixedRateBond(lag, 12345.0, schedule, [0.05], dc, q.Unadjusted, 100.0, reference, calls)
            else:
                bond = q.CallableZeroCouponBond(lag, 12345.0, q.NullCalendar(), maturity, dc, q.Unadjusted, 100.0, reference, calls)
            bond.setPricingEngine(engine)
            settlement = bond.settlementDate()
            discount = curve.discount(settlement)
            accrued = bond.accruedAmount(settlement)
            for index, (name, compounding) in enumerate(conventions):
                spread = [-0.004, 0.0, 0.015][len(rows) % 3]
                day_counter = q.Actual365Fixed() if index % 2 == 0 else q.Actual360()
                raw = bond.cleanPriceOAS(spread, handle, day_counter, compounding, q.Semiannual, settlement)
                normalized = (raw + accrued) / discount - accrued
                recovered = bond.OAS(raw, handle, day_counter, compounding, q.Semiannual, settlement, 1e-10, 100, 0.0)
                rows.append(dict(kind=kind, lag=lag, compounding=name, day_counter=365 if index % 2 == 0 else 360, spread=spread, native_clean=raw, normalized_clean=normalized, native_oas=recovered, settlement=settlement.serialNumber(), settlement_date=settlement.ISO(), discount=discount))
    return dict(quantlib_version=q.__version__, normalization="(native clean + market accrued) / unspread settlement discount - market accrued", cases=rows)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--raw-output", type=Path)
    args = parser.parse_args()
    data = json.dumps(generate(), sort_keys=True, indent=2) + "\n"
    if args.raw_output:
        args.raw_output.write_text(data)
    else:
        print(data, end="")


if __name__ == "__main__":
    main()
