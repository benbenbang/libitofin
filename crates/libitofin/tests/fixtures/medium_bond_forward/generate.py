"""Independent compiled QuantLib 1.43 oracle; no Rust implementation imported."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

import QuantLib as ql

PIN = "9863b578af0caa4cecabf697196533e84a8308b6"
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--source-root", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
assert ql.__version__ == "1.43"
assert subprocess.check_output(["git", "-C", str(args.source_root), "rev-parse", "HEAD"], text=True).strip() == PIN
paths = ["ql/instruments/forward.cpp", "ql/instruments/bondforward.cpp", "ql/cashflow.cpp"]
result = {"compiled_version": ql.__version__, "source_revision": PIN,
          "source_hashes": {p: hashlib.sha256((args.source_root / p).read_bytes()).hexdigest() for p in paths},
          "cases": []}
date = lambda y, m, d: ql.Date(d, m, y)
today = date(2025, 1, 2)
ql.Settings.instance().evaluationDate = today
cal, dc = ql.NullCalendar(), ql.Actual365Fixed()
spotq, finq, incq = (ql.SimpleQuote(r) for r in [.03, .04, .025])
spot, fin, inc = (ql.RelinkableYieldTermStructureHandle(ql.FlatForward(today, ql.QuoteHandle(q), dc)) for q in [spotq, finq, incq])

def bond(nominals, ex):
    starts = [date(2024, 7, 2), date(2025, 1, 2), date(2025, 7, 2), date(2026, 1, 2)]
    ends = starts[1:] + [date(2026, 7, 2)]
    coupons = [ql.FixedRateCoupon(end, n, .05, dc, start, end, ql.Date(), ql.Date(), end - 7 if ex else ql.Date()) for start, end, n in zip(starts, ends, nominals)]
    b = ql.Bond(2, cal, date(2024, 7, 2), coupons)
    b.setPricingEngine(ql.DiscountingBondEngine(spot))
    return b

def record(name, b, value=today, delivery=date(2025, 7, 2), days=0, short=False):
    f = ql.BondForward(value, delivery, ql.Position.Short if short else ql.Position.Long, 105., days, dc, cal, ql.Unadjusted, b, fin, inc)
    result["cases"].append({"name": name, "evaluation": ql.Settings.instance().evaluationDate.ISO(),
                            "forward": f.forwardPrice(), "clean": f.cleanForwardPrice(), "npv": f.NPV(),
                            "spot": b.dirtyPrice(), "income": f.spotIncome(inc), "settlement": f.settlementDate().ISO()})

standard = bond([100.] * 4, False)
record("before_coupon", standard, delivery=date(2025, 7, 1))
record("on_coupon", standard)
record("short", standard, short=True)
record("value_floor", standard, value=date(2025, 7, 2), delivery=date(2025, 7, 3))
record("face_250", bond([250.] * 4, False))
record("amortizing", bond([100., 80., 60., 40.], False))
record("delivery_redemption", standard, delivery=date(2026, 7, 2))
record("ex_coupon", bond([100.] * 4, True), value=date(2025, 6, 27), delivery=date(2025, 7, 3))
ql.Settings.instance().evaluationDate = date(2025, 6, 27)
record("evaluation_ex_coupon", bond([100.] * 4, True), delivery=date(2025, 7, 3))
ql.Settings.instance().evaluationDate = date(2025, 7, 2)
ql.Settings.instance().includeTodaysCashFlows = True
record("today_income_override", bond([100.] * 4, False), delivery=date(2026, 1, 3))
ql.Settings.instance().includeTodaysCashFlows = False
record("today_income_excluded", bond([100.] * 4, False), delivery=date(2026, 1, 3))
ql.Settings.instance().includeTodaysCashFlows = None
ql.Settings.instance().evaluationDate = today
spotq.setValue(.06)
record("spot_quote_update", standard)
finq.setValue(.07)
record("finance_quote_update", standard)
inc.linkTo(ql.FlatForward(today, .08, dc))
record("income_relink", standard)
ql.Settings.instance().evaluationDate = date(2025, 7, 2)
record("settings_update", standard, delivery=date(2026, 1, 3))
args.output.write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps(result["cases"], indent=2))
