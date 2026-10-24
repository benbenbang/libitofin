"""Project qualified independent observations without querying a native library."""

import argparse
import datetime
import hashlib
import json
from pathlib import Path

SOURCE_SHA256 = "b128faae6f31cb821e851545b64637aaad3d9706c7267602006eb0e2c2308572"
ROOT = Path(__file__).resolve().parents[3]


def serial(value):
    return (datetime.date(*value) - datetime.date(1899, 12, 30)).days


def observation(value):
    return [int(value["ok"]), value.get("value", 0.0)]


def flattened(number, row):
    terms = row["terms"]
    fields = [number, int(terms["calendar"] == "WeekendsOnly"), terms["settlement_days"],
              serial(terms["evaluation_date"]), serial(terms["discount_curve_reference"]),
              serial(terms["forward_curve_reference"]), terms["discount_quote"],
              terms["forward_quote"], int(terms["at_par"]), terms["ex_coupon_days"],
              serial(terms["issue_date"])]
    fields += [serial(value) for value in terms["schedule_dates"]]
    fields += terms["notionals"] + terms["floating_spreads"]
    fields += [len(terms["fixings"])]
    for fixing in terms["fixings"]:
        fields += [serial(fixing["date"]), fixing["value"]]
    fields += [serial(row["maturity_date"]), serial(row["settlement_date"]), int(row["is_expired"])]
    for name in ["npv", "settlement_value", "clean_price", "dirty_price", "accrued", "notional"]:
        fields += observation(row[name])
    fields += [len(row["flows"])]
    for flow in row["flows"]:
        fields += [serial(flow["payment_date"]), int(flow["kind"] == "principal")]
        fields += observation(flow["amount"])
    fields += [len(row["redemptions"])]
    for flow in row["redemptions"]:
        fields += [serial(flow["date"])] + observation(flow["amount"])
    fields += [len(row["notionals_by_date"])]
    for query in row["notionals_by_date"]:
        fields += [serial(query["date"])] + observation(query["value"])
    for name in ["clean_yield", "dirty_yield", "clean_yield_inversion", "dirty_yield_inversion"]:
        fields += observation(row[name]) if name in row else [-1, 0.0]
    return "\t".join(format(value, ".17g") for value in fields)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    args = parser.parse_args()
    original = args.source.read_bytes()
    if hashlib.sha256(original).hexdigest() != SOURCE_SHA256:
        raise ValueError("qualified source hash mismatch")
    selected = [line for line in original.splitlines(keepends=True)
                if (lambda row: row.get("terms", {}).get("type", row.get("type")) == "floating")(json.loads(line))]
    output = b"".join(selected)
    rows = [json.loads(line) for line in selected]
    if len(rows) != 53:
        raise ValueError("expected 53 unchanged floating rows")
    paths = [ROOT / "crates/itofin-py/tests/fixtures/amortizing_floating_oracle.jsonl",
             ROOT / "sdk/go/testdata/amortizing_floating_oracle.jsonl"]
    for path in paths:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(output)
    records = [row for row in rows if "terms" in row]
    if len(records) != 52 or any(len(row["terms"]["notionals"]) != 4 or len(row["terms"]["floating_spreads"]) != 1 for row in records):
        raise ValueError("unsupported C fixture projection geometry")
    tsv = "amortizing-floating-native-v1: variable numeric columns documented in README.md\n"
    tsv += "\n".join(flattened(i, row) for i, row in enumerate(records)) + "\n"
    (Path(__file__).parent / "values.tsv").write_text(tsv)
    manifest = {"qualified_source_sha256": SOURCE_SHA256, "floating_rows": 53,
                "financial_rows": 52, "projection": "unchanged original JSONL line bytes; C TSV selects native values only",
                "floating_jsonl_sha256": hashlib.sha256(output).hexdigest(),
                "c_tsv_sha256": hashlib.sha256(tsv.encode()).hexdigest(),
                "native_values_recomputed": False}
    (Path(__file__).parent / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")


if __name__ == "__main__":
    main()
