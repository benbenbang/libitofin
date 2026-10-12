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

    def test_nested_native_exports_are_discovered_without_relaxing_validation(self):
        native = self.root / "crates/libitofin-ffi/src/lib.rs"
        nested = native.parent / "gjr_model_api/calibration.rs"
        nested.parent.mkdir()
        nested.write_text(native.read_text())
        native.write_text("")
        status, output = self.run_check("--baseline")
        self.assertFalse(status, output)
        nested.write_text("")
        status, output = self.run_check("--baseline")
        self.assertTrue(status)
        self.assertIn("missing C export itofin_old", output)
        nested.write_text(self.files["crates/libitofin-ffi/src/lib.rs"])
        (self.root / "crates/libitofin-ffi/include/itofin.h").write_text("")
        status, output = self.run_check("--baseline")
        self.assertTrue(status)
        self.assertIn("missing C export itofin_old", output)

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


    def base_fixture(self):
        stub = (
            "class Base:\n"
            "    def __new__(cls, forbidden: typing.NoReturn) -> Base: ...\n"
            "    def value(self) -> builtins.float: ...\n"
            "class Derived(Base):\n"
            "    def __new__(cls, x: builtins.float) -> Derived: ...\n"
        )
        (self.root / "crates/itofin-py/python/itofin/__init__.pyi").write_text(stub)
        (self.root / "sdk/go/api.go").write_text(
            "type Base struct {}\nfunc (b *Base) Value() {}\nfunc NewDerived() {}\n"
        )
        rows = [
            {"python": "itofin.Base", "status": "implemented", "go": "Base", "c": "itofin_old"},
            {"python": "itofin.Base.value", "status": "implemented", "go": "Base.Value", "c": "itofin_old"},
            {"python": "itofin.Derived.__new__", "status": "implemented", "go": "NewDerived", "c": "itofin_old"},
            {"python": "itofin.Base.__new__", "status": "language-specific", "go": "Base",
             "classification": "nonconstructible-base",
             "reason": "An impossible base constructor has an actual constructible bound subclass."},
        ]
        self.write_enum_rows(rows)
        return stub, rows

    def test_nonconstructible_base_is_classified_not_counted_as_factory(self):
        self.base_fixture()
        report_path = self.root / "report.json"
        status, output = self.run_check("--report", str(report_path))
        self.assertFalse(status, output)
        report = json.loads(report_path.read_text())
        self.assertNotIn("itofin.Base.__new__", report["mappings"])
        self.assertEqual(report["classified"], 1)
        self.assertEqual(report["missing"], [])

    def test_base_classification_cannot_replace_a_baseline_implementation(self):
        self.base_fixture()
        status, output = self.run_check("--baseline", baseline={"itofin.Base.__new__": "method"})
        self.assertTrue(status)
        self.assertIn("UNMAPPED: itofin.Base.__new__", output)

    def test_base_requires_exact_impossible_constructor_and_actual_subclass(self):
        for old, new in [
            ("forbidden: typing.NoReturn", "forbidden: builtins.float"),
            ("forbidden: typing.NoReturn", "forbidden: typing.NoReturn = None"),
            ("forbidden: typing.NoReturn", "*forbidden: typing.NoReturn"),
            ("-> Base:", "-> typing.Any:"),
            ("class Derived(Base):", "class Derived:"),
            ("def __new__(cls, x: builtins.float) -> Derived", "def __new__(cls, x: typing.NoReturn) -> Derived"),
            ("    def value", "    def __init__(self) -> None: ...\n    def value"),
        ]:
            with self.subTest(replacement=new):
                stub, _ = self.base_fixture()
                (self.root / "crates/itofin-py/python/itofin/__init__.pyi").write_text(stub.replace(old, new))
                status, output = self.run_check()
                self.assertTrue(status)
                self.assertIn("unsupported language-specific classification", output)

    def test_base_rejects_overloaded_real_and_impossible_constructor_in_either_order(self):
        impossible = "    def __new__(cls, forbidden: typing.NoReturn) -> Base: ...\n"
        real = "    @typing.overload\n    def __new__(cls, value: builtins.float) -> Base: ...\n"
        for overloads in [real + impossible, impossible + real]:
            with self.subTest(order=overloads):
                stub, _ = self.base_fixture()
                (self.root / "crates/itofin-py/python/itofin/__init__.pyi").write_text(stub.replace(impossible, overloads))
                status, output = self.run_check()
                self.assertTrue(status)
                self.assertIn("unsupported language-specific classification", output)
        stub, _ = self.base_fixture()
        (self.root / "crates/itofin-py/python/itofin/__init__.pyi").write_text(
            stub.replace(impossible, "    @typing.overload\n" + impossible)
        )
        status, output = self.run_check()
        self.assertTrue(status)
        self.assertIn("unsupported language-specific classification", output)

    def test_base_rejects_async_constructor_variants_and_async_initializer(self):
        impossible = "    def __new__(cls, forbidden: typing.NoReturn) -> Base: ...\n"
        async_real = "    async def __new__(cls, value: builtins.float) -> Base: ...\n"
        for constructors in [async_real + impossible, impossible + async_real,
                             impossible.replace("def __new__", "async def __new__"),
                             impossible + "    async def __init__(self) -> None: ...\n"]:
            with self.subTest(constructors=constructors):
                stub, _ = self.base_fixture()
                (self.root / "crates/itofin-py/python/itofin/__init__.pyi").write_text(stub.replace(impossible, constructors))
                status, output = self.run_check()
                self.assertTrue(status)
                self.assertIn("unsupported language-specific classification", output)

    def test_base_requires_real_go_struct_and_mapped_methods_and_constructor(self):
        for change, message in [
            ("struct", "missing Go base type"),
            ("method", "corresponding Go base methods are not mapped"),
            ("subclass", "constructible subclass constructor is not mapped"),
            ("native", "constructible subclass constructor is not mapped"),
            ("base", "corresponding Go type is not mapped"),
        ]:
            with self.subTest(change=change):
                _, rows = self.base_fixture()
                go = self.root / "sdk/go/api.go"
                if change == "struct":
                    go.write_text(go.read_text().replace("type Base struct {}", "type Base int32"))
                elif change == "method":
                    go.write_text(go.read_text().replace("func (b *Base) Value() {}", "func Value() {}"))
                elif change == "subclass":
                    rows.pop(2)
                elif change == "native":
                    rows[2]["c"] = []
                elif change == "base":
                    rows.pop(0)
                self.write_enum_rows(rows)
                status, output = self.run_check()
                self.assertTrue(status)
                self.assertIn(message, output)


if __name__ == "__main__":
    unittest.main()
