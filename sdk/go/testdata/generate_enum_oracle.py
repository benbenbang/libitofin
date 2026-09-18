"""Record enum integers from an installed Python binding, independently of Go.

Run with itofin 0.22.0 installed: python sdk/go/testdata/generate_enum_oracle.py.
Every declared member is read from the Python extension at runtime; constructor
rejection is checked before recording it. The Go test consumes the saved values.
"""
import ast
import importlib
import json
from pathlib import Path

import itofin


def oracle():
    root = Path(__file__).resolve().parents[3]
    python_root = root / "crates/itofin-py/python"
    values = {}
    enums = 0
    for path in sorted((python_root / "itofin").rglob("*.pyi")):
        module_name = ".".join(path.relative_to(python_root).parts[:-1])
        for node in ast.parse(path.read_text()).body:
            if not isinstance(node, ast.ClassDef):
                continue
            methods = {child.name for child in node.body if isinstance(child, ast.FunctionDef)}
            if not {"__new__", "__int__"} <= methods:
                continue
            enum = getattr(importlib.import_module(module_name), node.name)
            enums += 1
            for args in [(), (0,), ("invalid",)]:
                try:
                    enum(*args)
                except TypeError:
                    pass
                else:
                    raise AssertionError(f"{module_name}.{node.name} accepted construction {args}")
            for member, value in vars(enum).items():
                if isinstance(value, enum):
                    values[f"{module_name}.{node.name}.{member}"] = int(value)
    return {"enums": enums, "python_version": itofin.__version__, "values": dict(sorted(values.items()))}


if __name__ == "__main__":
    Path(__file__).with_name("enum_oracle.json").write_text(json.dumps(oracle(), indent=2, sort_keys=True) + "\n")
