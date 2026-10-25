"""Generate fixed-bond payment navigation fixtures from independent QuantLib 1.43."""
import csv
import hashlib
import json
import sys
from pathlib import Path

import QuantLib as q
import QuantLib._QuantLib as native


DEST = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parents[1] / "sdk/go/testdata/payment_inspection"
DEST.mkdir(parents=True, exist_ok=True)
start, end, today = q.Date(24, 3, 2026), q.Date(24, 3, 2028), q.Date(24, 9, 2026)
cal, dc = q.NullCalendar(), q.Actual360()
q.Settings.instance().evaluationDate = today
q.Settings.instance().includeReferenceDateEvents = False
q.Settings.instance().includeTodaysCashFlows = None
schedule = q.Schedule(start, end, q.Period(6, q.Months), cal, q.Unadjusted, q.Unadjusted, q.DateGeneration.Forward, False)
bond = q.FixedRateBond(2, 375.0, schedule, [.047, .058], dc, q.Unadjusted, 102.0, start, cal,
                       q.Period(7, q.Days), cal, q.Unadjusted, False)
leg = bond.cashflows()
rows = []
for setting in [None, False, True]:
    q.Settings.instance().includeTodaysCashFlows = setting
    for settlement in [start - 1, start, today - 8, today - 7, today - 1, today, today + 1, end - 1, end, end + 1]:
        for include in [False, True]:
            values = [settlement.ISO(), "unset" if setting is None else str(setting).lower(), str(include).lower()]
            for direction in ["previous", "next"]:
                date = getattr(q.CashFlows, direction + "CashFlowDate")(leg, include, settlement)
                amount = getattr(q.CashFlows, direction + "CashFlowAmount")(leg, include, settlement)
                values.extend(["-", "-"] if date == q.Date() else [date.ISO(), format(amount, ".17g")])
            values.append(str(q.BondFunctions.isTradable(bond, settlement)).lower())
            rows.append(values)
q.Settings.instance().includeTodaysCashFlows = None
with (DEST / "bond-native.tsv").open("w", newline="") as output:
    writer = csv.writer(output, delimiter="\t", lineterminator="\n")
    writer.writerow(["settlement", "include_today", "include_settlement", "previous_date", "previous_amount", "next_date", "next_amount", "tradable"])
    writer.writerows(rows)
metadata = {
    "inputs": {"start": start.ISO(), "end": end.ISO(), "evaluation_date": today.ISO(), "settlement_days": 2,
               "nominal": 375.0, "rates": [.047, .058], "redemption": 102.0, "frequency_months": 6,
               "calendar": "NullCalendar", "convention": "Unadjusted", "day_counter": "Actual360", "ex_coupon_days": 7},
    "start_date": q.BondFunctions.startDate(bond).ISO(), "maturity_date": bond.maturityDate().ISO(),
    "settlement_date": bond.settlementDate().ISO(), "flows": [{"date": f.date().ISO(), "amount": f.amount()} for f in leg],
    "rows": len(rows), "fixture_sha256": hashlib.sha256((DEST / "bond-native.tsv").read_bytes()).hexdigest(),
    "provenance": {"native_version": q.__version__, "native_sha256": hashlib.sha256(Path(native.__file__).read_bytes()).hexdigest(),
                   "source_reference_revision": "9863b578af0caa4cecabf697196533e84a8308b6", "source_is_wheel_build_provenance": False,
                   "generator_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest()},
    "qualifications": ["No matching payment is encoded as '-' for both date and amount, distinct from actual zero payments.",
                       "Amounts are actual currency units and include all contiguous same-date principal and coupon flows, even ex-coupon.",
                       "includeTodaysCashFlows can override the explicit includeSettlementDateFlows argument when settlement equals today."]}
(DEST / "bond-native.json").write_text(json.dumps(metadata, indent=2, sort_keys=True, allow_nan=False) + "\n")
print(f"Generated {len(rows)} independent native payment rows in {DEST}")
