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

    def enum_fixture(self):
        (self.root / "crates/itofin-py/python/itofin/__init__.pyi").write_text(
            "class Enum:\n"
            "    A: typing.ClassVar[Enum]\n"
            "    def __new__(cls, forbidden: typing.NoReturn) -> Enum: ...\n"
            "    def __int__(self) -> builtins.int: ...\n"
        )
        (self.root / "sdk/go/api.go").write_text("type Enum int32\nconst A Enum = 0\n")
        rows = [
            {"python": "itofin.Enum.A", "status": "implemented", "go": "A"},
            {"python": "itofin.Enum.__int__", "status": "implemented", "go": "Enum: integer conversion"},
            {"python": "itofin.Enum.__new__", "status": "language-specific", "go": "Enum",
             "classification": "nonconstructible-enum",
             "reason": "Python forbids construction; Go exposes typed integer constants."},
        ]
        self.write_enum_rows(rows)
        return rows

    def write_enum_rows(self, rows):
        (self.root / "docs/go-coverage/test.json").write_text(json.dumps({"entries": rows}))

    def test_classification_counts_are_separate_from_implemented_mappings(self):
        self.enum_fixture()
        report_path = self.root / "report.json"
        status, _ = self.run_check("--report", str(report_path))
        self.assertFalse(status)
        report = json.loads(report_path.read_text())
        self.assertEqual((report["mapped"], report["classified"], report["accounted"]), (3, 1, 4))
        self.assertNotIn("itofin.Enum.__new__", report["mappings"])
        self.assertEqual(report["missing"], [])

    def test_classification_cannot_replace_a_baseline_mapping(self):
        self.enum_fixture()
        status, output = self.run_check("--baseline", baseline={"itofin.Enum.__new__": "method"})
        self.assertTrue(status)
        self.assertIn("UNMAPPED: itofin.Enum.__new__", output)

    def test_only_explicit_nonconstructible_enum_declarations_are_classifiable(self):
        for replacement in [
            "def arbitrary(): ...\n",
            "class Enum:\n    def __new__(cls, n: int) -> Enum: ...\n",
            "class Enum:\n    A: typing.ClassVar[Enum]\n"
            "    def __new__(cls, n: typing.NoReturn) -> Enum: ...\n",
        ]:
            with self.subTest(stub=replacement):
                rows = self.enum_fixture()
                (self.root / "crates/itofin-py/python/itofin/__init__.pyi").write_text(replacement)
                if replacement.startswith("def"):
                    rows[-1]["python"] = "itofin.arbitrary"
                self.write_enum_rows(rows[-1:])
                status, output = self.run_check()
                self.assertTrue(status)
                self.assertIn("unsupported language-specific classification", output)

    def test_classifications_require_reason_kind_and_real_mapped_go_type(self):
        for key, value, message in [
            ("reason", " ", "missing classification reason"),
            ("classification", "unsupported", "unsupported language-specific classification"),
            ("go", None, "classification requires a Go enum type"),
            ("go", ["Enum"], "classification requires a Go enum type"),
            ("go", "Absent", "missing Go enum type"),
            ("go", "Enum: descriptive suffix", "classification requires a Go enum type"),
            ("status", "ignored", "unknown mapping status"),
        ]:
            with self.subTest(key=key):
                rows = self.enum_fixture()
                rows[-1][key] = value
                self.write_enum_rows(rows)
                status, output = self.run_check()
                self.assertTrue(status)
                self.assertIn(message, output)
        rows = self.enum_fixture()
        self.write_enum_rows([rows[0], rows[-1]])
        status, output = self.run_check()
        self.assertTrue(status)
        self.assertIn("corresponding Go type is not mapped", output)

    def test_duplicate_and_conflicting_classifications_fail(self):
        rows = self.enum_fixture()
        for extra, message in [
            (rows[-1], "duplicate language-specific classification"),
            ({"python": "itofin.Enum.__new__", "status": "implemented", "go": "Enum"},
             "conflicting implemented and language-specific mappings"),
        ]:
            with self.subTest(message=message):
                self.write_enum_rows(rows + [extra])
                status, output = self.run_check()
                self.assertTrue(status)
                self.assertIn(message, output)

    def test_new_unclassified_declarations_still_fail_full_audit(self):
        self.enum_fixture()
        path = self.root / "crates/itofin-py/python/itofin/__init__.pyi"
        path.write_text(path.read_text() + "def fresh(): ...\n")
        status, output = self.run_check()
        self.assertTrue(status)
        self.assertIn("UNMAPPED: itofin.fresh", output)


if __name__ == "__main__":
    unittest.main()
