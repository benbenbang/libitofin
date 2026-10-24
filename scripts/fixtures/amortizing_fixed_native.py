"""Regenerate independently chosen amortizing-fixed cases using QuantLib 1.43.

The PyPI wheel is a compiled numerical reference, not a build of the separately
inspected local source oracle at 9863b578af0caa4cecabf697196533e84a8308b6.
Run only in an isolated oracle environment, never importing itofin.
"""

import json
from pathlib import Path

import QuantLib as q


def snapshot(bond, curve):
    """Read native currency values, per-100 quotes and complete principal legs."""
    current = bond.notional()
    clean = bond.cleanPrice()
    forward_values = None
    if q.Settings.instance().evaluationDate < q.Date(19, 1, 2026):
        forward = q.BondForward(q.Date(18, 1, 2024), q.Date(19, 1, 2026), q.Position.Long, 105., 0, q.Actual360(), q.WeekendsOnly(), q.Unadjusted, bond, curve, curve)
        forward_values = [forward.NPV(), forward.forwardPrice(), forward.cleanForwardPrice()]
    return {
        "forward": forward_values,
        "values": [bond.NPV(), bond.settlementValue(), clean, bond.dirtyPrice(), bond.accruedAmount(), current],
        "settlement": bond.settlementDate().serialNumber(),
        "maturity": bond.maturityDate().serialNumber(),
        "expired": bond.isExpired(),
        "flows": [[flow.date().serialNumber(), flow.amount()] for flow in bond.cashflows()],
        "redemptions": [[flow.date().serialNumber(), flow.amount()] for flow in bond.redemptions()],
        "yield": bond.bondYield(q.BondPrice(clean, q.BondPrice.Clean), q.Actual360(), q.Compounded, q.Annual, bond.settlementDate(), 1e-12, 1000, .04) if current else None,
        "dated_notionals": [bond.notional(q.Date(day, month, year)) for day, month, year in [(16, 1, 2025), (17, 1, 2025), (18, 1, 2025), (18, 1, 2026), (19, 1, 2026), (20, 1, 2026), (17, 1, 2027), (18, 1, 2027), (19, 1, 2027)]],
    }


def generate():
    """Produce deterministic native rows with delayed payments and ex-coupons."""
    if q.__version__ != "1.43":
        raise RuntimeError("QuantLib 1.43 required")
    rows = []
    calendar = q.WeekendsOnly()
    dc = q.Actual360()
    schedule = q.Schedule(q.Date(17, 1, 2024), q.Date(17, 1, 2027), q.Period(q.Annual), calendar, q.Unadjusted, q.Unadjusted, q.DateGeneration.Backward, False)
    quote = q.SimpleQuote(.031)
    curve = q.YieldTermStructureHandle(q.FlatForward(q.Date(18, 1, 2024), q.QuoteHandle(quote), dc))
    dates = [(18, 1, 2024), (10, 1, 2025), (17, 1, 2025), (18, 1, 2025), (19, 1, 2026), (20, 1, 2026), (17, 1, 2027), (18, 1, 2027), (19, 1, 2027)]
    for pattern, notionals in {"declining": [1000., 750., 400.], "plateau": [1000., 1000., 400.], "bullet": [1000., 1000., 1000.]}.items():
        for ex_days in [0, 7]:
            for settlement_days in [0, 2]:
                for when in dates:
                    q.Settings.instance().evaluationDate = q.Date(*when)
                    q.Settings.instance().includeReferenceDateEvents = False
                    bond = q.AmortizingFixedRateBond(settlement_days, notionals, schedule, [.045, .052, .039], dc, q.Following, q.Date(17, 1, 2024), q.Period(ex_days, q.Days), calendar, q.Unadjusted, False)
                    bond.setPricingEngine(q.DiscountingBondEngine(curve))
                    quote.setValue(.031)
                    initial = snapshot(bond, curve)
                    quote.setValue(.067)
                    updated = snapshot(bond, curve)
                    rows.append({"name": f"{pattern}-ex{ex_days}-sd{settlement_days}-{when[2]}{when[1]:02}{when[0]:02}", "notionals": notionals, "ex_days": ex_days, "settlement_days": settlement_days, "when": list(when), "initial": initial, "updated": updated})
    return {"compiled_quantlib_version": q.__version__, "source_quantlib_commit": "9863b578af0caa4cecabf697196533e84a8308b6", "source_is_compiled_wheel": False, "rows": rows}


if __name__ == "__main__":
    destination = Path(__file__).resolve().parents[2] / "sdk/go/testdata/amortizing_fixed_native.jsonl"
    result = generate()
    rows = result.pop("rows")
    destination.write_text("\n".join(json.dumps(row, sort_keys=True, allow_nan=False) for row in [result, *rows]) + "\n")
