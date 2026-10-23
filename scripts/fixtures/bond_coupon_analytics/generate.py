"""Generate exact-binary64 BPS/ATM observations from compiled QuantLib, not Rust."""
import csv
import json
from pathlib import Path

import QuantLib as ql

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "sdk/go/testdata/bond_analytics_oracle.json"
COLUMNS = ["name", "kind", "compounding", "frequency", "face", "start", "end", "reference", "settlement", "yield", "bps_curve", "bps_yield", "atm_curve"]
MODES = [ql.Simple, ql.Compounded, ql.Continuous, ql.SimpleThenCompounded, ql.CompoundedThenSimple]
FREQUENCIES = [ql.Annual, ql.Semiannual, ql.Quarterly, ql.Monthly]


def observations() -> list[dict]:
    """Use the independently established Go analytics fixture construction contract."""
    if ql.__version__ != "1.43":
        raise RuntimeError("QuantLib 1.43 is required for this pinned producer")
    rows = []
    for source in json.loads(SOURCE.read_text())["cases"]:
        start, end, reference, settlement = [ql.Date(source[key]) for key in ["start", "end", "reference", "settlement"]]
        calendar, accrual = ql.NullCalendar(), ql.Actual360()
        curve = ql.FlatForward(reference, .032, ql.Actual365Fixed(), ql.Continuous, ql.Annual)
        if source["kind"] == "zero":
            bond = ql.ZeroCouponBond(5, calendar, source["face"], end, ql.Unadjusted, 102.0, start)
        else:
            schedule = ql.Schedule(start, end, ql.Period(ql.Semiannual), calendar, ql.Unadjusted, ql.Unadjusted, ql.DateGeneration.Backward, False)
            options = {"exCouponPeriod": ql.Period(7, ql.Days), "exCouponCalendar": calendar} if source["kind"] == "ex_coupon" else {}
            bond = ql.FixedRateBond(5, source["face"], schedule, [.047, .058], accrual, ql.Unadjusted, 102.0, start, **options)
        rows.append({**{key: source[key] for key in COLUMNS[:10]},
                     "bps_curve": ql.BondFunctions.bps(bond, curve, settlement),
                     "bps_yield": ql.BondFunctions.bps(bond, source["yield"], accrual, MODES[source["compounding"]], FREQUENCIES[source["frequency"]], settlement),
                     "atm_curve": ql.BondFunctions.atmRate(bond, curve, settlement)})
    if len(rows) != 55:
        raise RuntimeError("Expected exactly 55 established input cases")
    return rows


if __name__ == "__main__":
    rows = observations()
    for path in [ROOT / "crates/itofin-py/tests/fixtures/bond_coupon_analytics.tsv", ROOT / "sdk/go/testdata/bond_coupon_analytics.tsv"]:
        with path.open("w", newline="") as stream:
            writer = csv.DictWriter(stream, fieldnames=COLUMNS, delimiter="\t", lineterminator="\n")
            writer.writeheader()
            writer.writerows(rows)
    print(f"Generated {len(rows)} rows from compiled QuantLib {ql.__version__}")
