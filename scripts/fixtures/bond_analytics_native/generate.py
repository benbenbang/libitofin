"""Regenerate bond_analytics_native.json expectations from the compiled QuantLib wheel, not Rust."""
import hashlib
import json
import sys
from pathlib import Path

import QuantLib as ql

VERSION = "1.44"
ROOT = Path(__file__).resolve().parents[3]
TARGET = ROOT / "crates/itofin-py/tests/fixtures/bond_analytics_native.json"
COMPOSITE = {"SimpleThenCompounded", "CompoundedThenSimple"}


def date(parts):
    return ql.Date(*parts)


def unsupported(call):
    """Composite compoundings have no direct curve-spread calculation from QuantLib 1.44 on."""
    try:
        return call()
    except RuntimeError as error:
        if "not supported for direct calculations" not in str(error):
            raise
        return None


def build(row):
    calendar, dc = ql.NullCalendar(), ql.Actual360()
    if row["kind"] == "zero":
        return ql.ZeroCouponBond(row["settlement_days"], calendar, row["face"], date(row["end"]), ql.Unadjusted, row["redemption"])
    schedule = ql.Schedule(date(row["start"]), date(row["end"]), ql.Period(ql.Semiannual), calendar, ql.Unadjusted, ql.Unadjusted, ql.DateGeneration.Backward, False)
    ex = ql.Period(row["ex_days"], ql.Days) if row["ex_days"] else ql.Period()
    return ql.FixedRateBond(row["settlement_days"], row["face"], schedule, row["coupons"], dc, ql.Unadjusted, row["redemption"], ql.Date(), calendar, ex, calendar)


def expected(row, accuracy, evaluations):
    ql.Settings.instance().evaluationDate = date(row["eval"])
    bond, dc = build(row), ql.Actual360()
    settlement, reference = date(row["settlement"]), date(row["reference"])
    curve = ql.FlatForward(reference, row["curve_rate"], dc)
    mode = getattr(ql, row["compounding"])
    frequency = getattr(ql, row["frequency"])
    y, z = row["yield_rate"], row["z_spread"]
    accrued = bond.accruedAmount(settlement)
    clean_yield = ql.BondFunctions.cleanPrice(bond, y, dc, mode, frequency, settlement)
    out = {
        "accrued": accrued,
        "clean_curve": ql.BondFunctions.cleanPrice(bond, curve, settlement),
        "dirty_curve": ql.BondFunctions.dirtyPrice(bond, curve, settlement),
        "clean_yield": clean_yield,
        "dirty_yield": clean_yield + accrued,
        "notional": bond.notional(settlement),
        "flows": [{"amount": f.amount(), "date": [f.date().dayOfMonth(), f.date().month(), f.date().year()]} for f in bond.cashflows()],
    }
    for suffix in ["clean", "dirty"]:
        price = ql.BondPrice(out[suffix + "_yield"], getattr(ql.BondPrice, suffix.capitalize()))
        out["yield_from_" + suffix] = ql.BondFunctions.bondYield(bond, price, dc, mode, frequency, settlement, accuracy, evaluations, 0.05)
    out["clean_z_spread"] = unsupported(lambda: ql.BondFunctions.cleanPrice(bond, curve, z, mode, frequency, settlement))
    out["dirty_z_spread"] = unsupported(lambda: ql.BondFunctions.dirtyPrice(bond, curve, z, mode, frequency, settlement))
    for suffix in ["clean", "dirty"]:
        target = out[suffix + "_z_spread"]
        out["z_spread_from_" + suffix] = None if target is None else ql.BondFunctions.zSpread(
            bond, ql.BondPrice(target, getattr(ql.BondPrice, suffix.capitalize())), curve, mode, frequency, settlement, accuracy, evaluations, 0.0)
    leg = bond.cashflows()
    npv = unsupported(lambda: ql.CashFlows.npv(leg, curve, z, dc, mode, frequency, False, settlement, reference))
    out["leg_npv_distinct_npv_date"] = npv
    out["leg_z_spread"] = None if npv is None else ql.CashFlows.zSpread(leg, npv, curve, dc, mode, frequency, False, settlement, reference, accuracy, evaluations, 0.0)
    return {key: out[key] for key in sorted(out)}


def main():
    if ql.__version__ != VERSION and "--any-version" not in sys.argv:
        raise RuntimeError(f"QuantLib {VERSION} is required")
    fixture = json.loads(TARGET.read_text())
    accuracy, evaluations = fixture["solver_fixture_accuracy"], fixture["solver_fixture_max_evaluations"]
    for row in fixture["cases"]:
        row["expected"] = {**row["expected"], **expected(row, accuracy, evaluations)}
    fixture["oracle"] = f"QuantLib {ql.__version__}"
    fixture["source_generator_sha256"] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    TARGET.write_text(json.dumps(fixture, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
