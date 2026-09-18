import contextlib
import io
import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import check_go_coverage as coverage


class BaselineCoverageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.files = {
            "crates/itofin-py/python/itofin/__init__.pyi": "def old(): ...\ndef new(): ...\n",
            "crates/libitofin-ffi/src/lib.rs": "fn itofin_old() {}\n",
            "crates/libitofin-ffi/include/itofin.h": "int itofin_old();\n",
            "sdk/go/api.go": "func Old() {}\n",
            "sdk/go/api_test.go": "func TestOld() {}\n",
            "docs/go-coverage/test.json": json.dumps({"entries": [{
                "python": "itofin.old", "status": "implemented", "c": "itofin_old",
                "go": "Old", "tests": ["TestOld"],
            }]}),
        }
        for name, text in self.files.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
        self.addCleanup(patch.stopall)
        patch.object(coverage, "ROOT", self.root).start()
        self.inventory = coverage.inventory

    def run_check(self, *args, baseline=None):
        def inventory(ref=None):
            if ref is None:
                return self.inventory()
            return {"itofin.old": "function"} if baseline is None else baseline

        output = io.StringIO()
        with patch.object(coverage, "inventory", side_effect=inventory), \
                patch("sys.argv", ["check_go_coverage.py", "--strict", *args]), \
                contextlib.redirect_stdout(output):
            status = coverage.main()
        return status, output.getvalue()

    def test_new_symbols_are_reported_without_failing_baseline(self):
        report_path = self.root / "report.json"
        status, output = self.run_check("--baseline", "--report", str(report_path))
        self.assertFalse(status)
        self.assertIn("NEW UNMAPPED (outside baseline): itofin.new", output)
        report = json.loads(report_path.read_text())
        self.assertEqual(report["baseline"]["mapped"], 1)
        self.assertEqual(report["baseline"]["missing"], [])
        self.assertEqual(report["baseline"]["newer_unmapped"], ["itofin.new"])

    def test_full_current_parity_still_fails(self):
        status, output = self.run_check()
        self.assertTrue(status)
        self.assertIn("UNMAPPED: itofin.new", output)
        self.assertNotIn("outside baseline", output)

    def test_removed_baseline_mapping_fails(self):
        (self.root / "docs/go-coverage/test.json").write_text('{"entries": []}')
        status, output = self.run_check("--baseline")
        self.assertTrue(status)
        self.assertIn("UNMAPPED: itofin.old", output)

    def test_removed_baseline_python_symbol_fails(self):
        (self.root / "crates/itofin-py/python/itofin/__init__.pyi").write_text("def new(): ...\n")
        status, output = self.run_check("--baseline")
        self.assertTrue(status)
        self.assertIn("UNMAPPED: itofin.old", output)

    def test_invalid_implementation_and_test_references_still_fail(self):
        for name, message in [
            ("crates/libitofin-ffi/src/lib.rs", "missing C export"),
            ("crates/libitofin-ffi/include/itofin.h", "missing C export"),
            ("sdk/go/api.go", "missing Go identifier"),
            ("sdk/go/api_test.go", "missing test"),
        ]:
            with self.subTest(name=name):
                path = self.root / name
                path.write_text("")
                status, output = self.run_check("--baseline")
                self.assertTrue(status)
                self.assertIn(message, output)
                path.write_text(self.files[name])

    def test_empty_baseline_fails_closed(self):
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as error:
            self.run_check("--baseline", baseline={})
        self.assertEqual(error.exception.code, 2)

    def test_unavailable_baseline_fails_closed(self):
        with patch.object(coverage.subprocess, "check_output", side_effect=subprocess.CalledProcessError(128, "git")), \
                patch("sys.argv", ["check_go_coverage.py", "--strict", "--baseline"]), \
                contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as error:
            coverage.main()
        self.assertEqual(error.exception.code, 2)


if __name__ == "__main__":
    unittest.main()
