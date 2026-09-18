"""Regression tests for the bidirectional typing gate."""

import json
import subprocess
import sys
import tempfile
import unittest
from copy import deepcopy
from pathlib import Path

import check_typing as gate


class TypingGateTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.gettempdir()).resolve() / "typing-gate-fixture"
        self.record = {
            "file": str(self.root / "test.py"),
            "severity": "error",
            "rule": "reportOperatorIssue",
            "message": 'Operator "-" not supported',
            "range": {"start": {"line": 10, "character": 4}},
        }
        self.report = {"version": "1.1.411", "generalDiagnostics": [self.record]}
        self.key = next(iter(gate.normalize(self.report, self.root)))
        self.baseline = {
            "pyright_version": "1.1.411",
            "diagnostics": [dict(zip(gate.FIELDS, self.key)) | {"count": 1}],
            "accepted_removals": [],
        }

    def test_additions_and_disappearances_both_fail(self):
        self.report["generalDiagnostics"] = []
        added, missing = gate.compare(self.report, self.baseline, self.root)
        self.assertFalse(added)
        self.assertEqual(missing[self.key], 1)
        self.record["message"] = "A different diagnostic"
        self.report["generalDiagnostics"] = [self.record]
        added, missing = gate.compare(self.report, self.baseline, self.root)
        self.assertEqual(sum(added.values()), 1)
        self.assertEqual(sum(missing.values()), 1)

    def test_duplicate_occurrences_are_not_collapsed(self):
        self.report["generalDiagnostics"].append(deepcopy(self.record))
        added, missing = gate.compare(self.report, self.baseline, self.root)
        self.assertEqual(added[self.key], 1)
        self.assertFalse(missing)
        self.baseline["diagnostics"][0]["count"] = 3
        added, missing = gate.compare(self.report, self.baseline, self.root)
        self.assertFalse(added)
        self.assertEqual(missing[self.key], 1)

    def test_lines_whitespace_and_checkout_location_are_stable(self):
        self.record["range"]["start"]["line"] = 900
        self.record["message"] = 'Operator  "-"\n not supported'
        self.record["file"] = str(self.root / "other-checkout/test.py")
        self.assertEqual(gate.compare(self.report, self.baseline, self.root / "other-checkout"), ({}, {}))

    def test_file_rule_and_severity_are_identity(self):
        for field, value in [("file", str(self.root / "other.py")), ("rule", "otherRule"), ("severity", "warning")]:
            with self.subTest(field=field):
                report = deepcopy(self.report)
                report["generalDiagnostics"][0][field] = value
                added, missing = gate.compare(report, self.baseline, self.root)
                self.assertEqual(sum(added.values()), 1)
                self.assertEqual(sum(missing.values()), 1)

    def removal(self):
        return {
            "id": gate.diagnostic_id(self.key),
            "count": 1,
            "classification": "accuracy-improvement",
            "reason": "Sequence accepts covariant helper lists",
            "evidence": "Reviewed source and runtime test",
        }

    def test_classified_removal_is_explicit_and_cannot_hide_returning_error(self):
        self.baseline["accepted_removals"] = [self.removal()]
        added, missing = gate.compare(self.report, self.baseline, self.root)
        self.assertEqual(added[self.key], 1)
        self.assertFalse(missing)
        self.report["generalDiagnostics"] = []
        self.assertEqual(gate.compare(self.report, self.baseline, self.root), ({}, {}))

    def test_bad_acceptance_records_fail_closed(self):
        for update in [
            {"id": "unknown"},
            {"count": 0},
            {"count": 2},
            {"count": True},
            {"classification": "unreviewed"},
            {"reason": ""},
            {"evidence": ""},
            {"classification": "accepted-precision-loss"},
        ]:
            with self.subTest(update=update):
                self.baseline["accepted_removals"] = [self.removal() | update]
                with self.assertRaises(ValueError):
                    gate.expected_diagnostics(self.baseline)
        self.baseline["accepted_removals"] = [self.removal(), self.removal()]
        with self.assertRaises(ValueError):
            gate.expected_diagnostics(self.baseline)

    def test_precision_loss_rejects_issue_prefix_without_positive_number(self):
        for suffix in ("", "0", "pending"):
            self.baseline["accepted_removals"] = [
                self.removal()
                | {
                    "classification": "accepted-precision-loss",
                    "follow_up": "https://github.com/benbenbang/libitofin/issues/" + suffix,
                }
            ]
            with self.assertRaises(ValueError):
                gate.expected_diagnostics(self.baseline)

    def test_precision_loss_requires_follow_up(self):
        self.baseline["accepted_removals"] = [
            self.removal()
            | {
                "classification": "accepted-precision-loss",
                "follow_up": "https://github.com/benbenbang/libitofin/issues/993",
            }
        ]
        self.assertFalse(gate.expected_diagnostics(self.baseline))

    def test_version_mismatch_and_outside_source_are_rejected(self):
        self.report["version"] = "unexpected"
        with self.assertRaises(ValueError):
            gate.compare(self.report, self.baseline, self.root)
        self.record["file"] = str(self.root.parent / "outside.py")
        with self.assertRaises(ValueError):
            gate.normalize(self.report, self.root)

    def test_historical_invariance_removals_preserve_date_subtraction(self):
        baseline = json.loads(gate.BASELINE.read_text())
        historical = [record for record in baseline["accepted_removals"] if record["evidence"].startswith("#625:")]
        self.assertEqual(len(historical), 9)
        self.assertTrue(all(record["classification"] == "accuracy-improvement" for record in historical))
        expected = gate.expected_diagnostics(baseline)
        date_errors = [key for key in expected if "Literal['a week']" in key[3]]
        self.assertEqual(len(date_errors), 1)

    def test_command_exit_status_checks_both_directions(self):
        baseline = json.loads(gate.BASELINE.read_text())
        records = [
            dict(zip(gate.FIELDS, key)) | {"file": str(gate.ROOT / key[0])}
            for key, count in gate.expected_diagnostics(baseline).items()
            for _ in range(count)
        ]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "report.json"
            for diagnostics, expected_code in [(records, 0), (records[:-1], 1), (records + records[:1], 1)]:
                path.write_text(json.dumps({"version": baseline["pyright_version"], "generalDiagnostics": diagnostics}))
                result = subprocess.run(
                    [sys.executable, str(Path(gate.__file__)), "--report", str(path)], capture_output=True, text=True
                )
                self.assertEqual(result.returncode, expected_code, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
