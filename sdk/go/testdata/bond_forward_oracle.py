"""Generate BondForward fixtures using compiled QuantLib 1.43 only."""
import argparse
import hashlib
import json
import subprocess
from pathlib import Path
import QuantLib as ql

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--source-root", type=Path, required=True)
args = parser.parse_args()
assert ql.__version__ == "1.43"
pin = "9863b578af0caa4cecabf697196533e84a8308b6"
assert subprocess.check_output(["git", "-C", str(args.source_root), "rev-parse", "HEAD"], text=True).strip() == pin
source_paths = ["ql/instruments/bondforward.cpp", "ql/instruments/forward.cpp", "ql/instruments/bonds/fixedratebond.cpp", "ql/instruments/bonds/zerocouponbond.cpp"]
provenance = dict(version=ql.__version__, source_revision=pin, source_hashes={p: hashlib.sha256((args.source_root / p).read_bytes()).hexdigest() for p in source_paths})
today = ql.Date(2, 1, 2025)
ql.Settings.instance().evaluationDate = today
cal, dc = ql.NullCalendar(), ql.Actual365Fixed()
spot, finance, income = [ql.YieldTermStructureHandle(ql.FlatForward(today, r, dc)) for r in (.03, .04, .025)]
rows = []
for kind in ("fixed", "zero"):
    for face in (100., 250.):
        for delivery in (ql.Date(1, 7, 2025), ql.Date(2, 7, 2025), ql.Date(2, 7, 2026)):
            for separate in (False, True):
                for short in (False, True):
                    if kind == "fixed":
                        schedule = ql.Schedule(ql.Date(2, 7, 2024), ql.Date(2, 7, 2026), ql.Period(ql.Semiannual), cal, ql.Unadjusted, ql.Unadjusted, ql.DateGeneration.Backward, False)
                        bond = ql.FixedRateBond(2, face, schedule, [.05], dc, ql.Unadjusted, 100., ql.Date(2, 7, 2024))
                    else:
                        bond = ql.ZeroCouponBond(2, cal, face, ql.Date(2, 7, 2026), ql.Unadjusted, 100., ql.Date(2, 7, 2024))
                    bond.setPricingEngine(ql.DiscountingBondEngine(spot))
                    forward = ql.BondForward(today, delivery, ql.Position.Short if short else ql.Position.Long, 105., 0, dc, cal, ql.Unadjusted, bond, finance, income if separate else finance)
                    rows.append(dict(kind=kind, face=face, delivery=delivery.serialNumber(), separate=separate, short=short, values=[forward.NPV(),forward.forwardPrice(),forward.cleanForwardPrice()], settlement=forward.settlementDate().serialNumber()))
live_quotes = [ql.SimpleQuote(r) for r in (.03, .04, .025)]
live_curves = [ql.YieldTermStructureHandle(ql.FlatForward(today, ql.QuoteHandle(q), dc)) for q in live_quotes]
schedule = ql.Schedule(ql.Date(2, 7, 2024), ql.Date(2, 7, 2026), ql.Period(ql.Semiannual), cal, ql.Unadjusted, ql.Unadjusted, ql.DateGeneration.Backward, False)
live_bond = ql.FixedRateBond(2, 100., schedule, [.05], dc, ql.Unadjusted, 100., ql.Date(2, 7, 2024))
live_bond.setPricingEngine(ql.DiscountingBondEngine(live_curves[0]))
live_forwards = [ql.BondForward(today, ql.Date(2, 7, 2025), ql.Position.Long, 105., 0, dc, cal, ql.Unadjusted, live_bond, live_curves[1], live_curves[2] if separate else live_curves[1]) for separate in (False, True)]
live_rows = []
for stage in ("base", "finance", "income", "spot"):
    if stage != "base":
        index, rate = {"finance": (1, .07), "income": (2, .08), "spot": (0, .06)}[stage]
        live_quotes[index].setValue(rate)
    live_rows.append(dict(stage=stage, values=[[f.NPV(), f.forwardPrice(), f.cleanForwardPrice()] for f in live_forwards]))
boundary = ql.BondForward(ql.Date(7, 4, 2025), ql.Date(5, 7, 2025), ql.Position.Long, 105., 2, dc, ql.WeekendsOnly(), ql.Following, live_bond, live_curves[1], live_curves[2])
provenance["live_rows"] = live_rows
provenance["boundary"] = dict(settlement=boundary.settlementDate().serialNumber(), delivery=ql.Date(7, 7, 2025).serialNumber(), values=[boundary.NPV(), boundary.forwardPrice(), boundary.cleanForwardPrice()])
path = Path(__file__).with_name("bond_forward_native.json")
provenance["rows"] = rows
path.write_text(json.dumps(provenance, indent=2, sort_keys=True) + "\n")
print(path, len(rows))
