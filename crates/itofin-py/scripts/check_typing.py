#!/usr/bin/env python3
"""Require reviewed Pyright diagnostics, including explicitly accepted removals."""

import argparse
import hashlib
import json
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
CONFIG = ROOT / "crates/itofin-py/typing-config.json"
BASELINE = ROOT / "crates/itofin-py/typing-baseline.json"
FIELDS = ("file", "severity", "rule", "message")


def identity(record):
    return tuple(record[field] for field in FIELDS)


def diagnostic_id(key):
    return hashlib.sha256(json.dumps(key, ensure_ascii=True).encode()).hexdigest()[:16]


def normalize(report, root):
    diagnostics = Counter()
    for record in report["generalDiagnostics"]:
        path = Path(record["file"]).resolve().relative_to(root.resolve()).as_posix()
        diagnostics[(path, record["severity"], record.get("rule", ""), " ".join(record["message"].split()))] += 1
    return diagnostics


def expected_diagnostics(baseline):
    expected = Counter()
    identifiers = {}
    for record in baseline["diagnostics"]:
        key = identity(record)
        count = record["count"]
        if key in expected or type(count) is not int or count < 1:
            raise ValueError("Baseline diagnostics must be unique with positive integer counts")
        expected[key] = count
        identifiers[diagnostic_id(key)] = key
    seen = set()
    for removal in baseline["accepted_removals"]:
        identifier = removal["id"]
        if identifier in seen or identifier not in identifiers:
            raise ValueError(f"Duplicate or unknown accepted removal: {identifier}")
        seen.add(identifier)
        classification = removal["classification"]
        if classification not in ("accuracy-improvement", "accepted-precision-loss"):
            raise ValueError(f"Unclassified removal: {identifier}")
        if not removal.get("reason", "").strip() or not removal.get("evidence", "").strip():
            raise ValueError(f"Removal requires a reason and evidence: {identifier}")
        if classification == "accepted-precision-loss" and not re.fullmatch(
            r"https://github\.com/benbenbang/libitofin/issues/[1-9][0-9]*", removal.get("follow_up", "")
        ):
            raise ValueError(f"Precision loss requires a filed follow-up: {identifier}")
        key = identifiers[identifier]
        count = removal["count"]
        if type(count) is not int or not 1 <= count <= expected[key]:
            raise ValueError(f"Invalid accepted removal count: {identifier}")
        expected[key] -= count
    return +expected


def compare(report, baseline, root):
    if report["version"] != baseline["pyright_version"]:
        raise ValueError(f"Expected Pyright {baseline['pyright_version']}, got {report['version']}")
    expected = expected_diagnostics(baseline)
    actual = normalize(report, root)
    return actual - expected, expected - actual


def describe(label, diagnostics):
    for key, count in sorted(diagnostics.items()):
        print(f"{label} ({count} occurrence(s), id={diagnostic_id(key)}):")
        print(json.dumps(dict(zip(FIELDS, key)), indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, help="Compare an existing Pyright JSON report")
    args = parser.parse_args()
    try:
        baseline = json.loads(BASELINE.read_text())
        if args.report:
            report = json.loads(args.report.read_text())
        else:
            result = subprocess.run(
                [
                    sys.executable,
                    "-m",
                    "pyright",
                    "--project",
                    str(CONFIG),
                    "--pythonpath",
                    sys.executable,
                    "--outputjson",
                ],
                cwd=ROOT,
                capture_output=True,
                text=True,
                check=False,
            )
            if result.returncode not in (0, 1):
                raise ValueError(f"Pyright failed ({result.returncode}): {result.stderr or result.stdout}")
            report = json.loads(result.stdout)
        added, missing = compare(report, baseline, ROOT)
        describe("NEW diagnostic", added)
        describe("MISSING diagnostic; classify the disappearance before accepting it", missing)
        if added or missing:
            return 1
        print(f"Pyright {report['version']}: reviewed diagnostic multiset matches in both directions")
        return 0
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"Typing gate error: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
