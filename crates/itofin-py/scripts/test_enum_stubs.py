"""Exercise generated enum postprocessing and its static public contract."""

import ast
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import gen_stubs


class EnumStubTests(unittest.TestCase):
    """Guard member accuracy without granting Python Enum capabilities."""

    def test_postprocessing_is_idempotent(self) -> None:
        """Preserve docs and other classes while removing the false base."""
        source = '''import builtins
import enum
import typing
@typing.final
class Choice(enum.Enum):
    """First choice."""
    First = ...
class Other:
    def __new__(cls, value: int) -> Other: ...
'''
        result = gen_stubs.post_process(source)
        self.assertIn("First: typing.ClassVar[Choice]", result)
        self.assertIn('"""First choice."""', result)
        self.assertNotIn("import enum", result)
        self.assertEqual(result, gen_stubs.post_process(result))
        with tempfile.TemporaryDirectory() as directory:
            stub = Path(directory) / "choice.pyi"
            stub.write_text(result)
            checked = subprocess.run(
                [sys.executable, "-m", "pyright", str(stub), "--pythonversion", "3.10", "--outputjson"],
                capture_output=True, text=True, check=False,
            )
            self.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)
            self.assertEqual(json.loads(checked.stdout)["generalDiagnostics"], [])
        with self.assertRaisesRegex(ValueError, "unsupported generated enum member"):
            gen_stubs.post_process(source.replace("First = ...", "First = 1"))

    def test_static_enum_contract(self) -> None:
        """Accept all typed members and reject absent Enum APIs and constructors."""
        imports = ["from collections.abc import Hashable"]
        accepted = []
        rejected = []
        for path in sorted(gen_stubs.PKG_DIR.glob("*/__init__.pyi")):
            for cls in ast.parse(path.read_text()).body:
                if not isinstance(cls, ast.ClassDef):
                    continue
                members = [
                    member.target.id for member in cls.body
                    if isinstance(member, ast.AnnAssign)
                    and isinstance(member.target, ast.Name)
                    and ast.unparse(member.annotation) == f"typing.ClassVar[{cls.name}]"
                ]
                if not members:
                    continue
                imports.append(f"from itofin.{path.parent.name} import {cls.name}")
                for name in members:
                    accepted.append(f"v_{cls.name}_{name}: {cls.name} = {cls.name}.{name}")
                instance = f"{cls.name}.{members[0]}"
                accepted.extend([f"i_{cls.name}: int = int({instance})", f"b_{cls.name}: bool = {instance} == 0"])
                rejected.extend([
                    f"{instance}.value", f"{instance}.name", f"iter({cls.name})",
                    f"{cls.name}()", f"{cls.name}(0)", f"{instance}.{members[0]} = {instance}",
                    f"h_{cls.name}: Hashable = {instance}",
                ])
        self.assertEqual(len(imports), 25)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture = root / "enum_contract.py"
            config = root / "pyrightconfig.json"
            config.write_text(json.dumps({
                "include": [fixture.name], "extraPaths": [str(gen_stubs.PKG_DIR.parent)],
                "pythonVersion": "3.10", "typeCheckingMode": "basic",
            }))
            for statements, expect_errors in [(accepted, False), (rejected, True)]:
                fixture.write_text("\n".join(imports + statements) + "\n")
                result = subprocess.run(
                    [sys.executable, "-m", "pyright", "--project", str(config), "--outputjson"],
                    capture_output=True, text=True, check=False,
                )
                self.assertIn(result.returncode, (0, 1), result.stderr)
                diagnostics = json.loads(result.stdout)["generalDiagnostics"]
                if expect_errors:
                    self.assertEqual(
                        {item["range"]["start"]["line"] for item in diagnostics},
                        set(range(len(imports), len(imports) + len(statements))),
                    )
                else:
                    self.assertEqual(diagnostics, [])


if __name__ == "__main__":
    unittest.main()
