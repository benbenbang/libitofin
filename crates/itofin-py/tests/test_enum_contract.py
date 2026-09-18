"""Match fieldless-enum stubs to the installed PyO3 classes."""

import ast
import enum
import importlib
from pathlib import Path
from typing import Any

import itofin
import pytest


def test_fieldless_enum_runtime_contract() -> None:
    """Every exposed enum member is a nonconstructible integer-convertible object."""
    package = Path(itofin.__file__).parent
    checked = set()
    for path in package.glob("*/__init__.pyi"):
        module = importlib.import_module(f"itofin.{path.parent.name}")
        declarations = {node.name: node for node in ast.parse(path.read_text()).body if isinstance(node, ast.ClassDef)}
        for name, cls in vars(module).items():
            if not isinstance(cls, type) or "__int__" not in vars(cls):
                continue
            members = {name: value for name, value in vars(cls).items() if isinstance(value, cls)}
            if not members:
                continue
            checked.add(cls.__name__)
            assert not issubclass(cls, enum.Enum)
            declaration = declarations[name]
            assert declaration.bases == []
            annotations = {
                node.target.id: ast.unparse(node.annotation)
                for node in declaration.body
                if isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name)
            }
            assert annotations.pop("__hash__") == "typing.ClassVar[None]"
            assert annotations == {member: f"typing.ClassVar[{name}]" for member in members}
            dynamic_cls: Any = cls
            with pytest.raises(TypeError):
                dynamic_cls()
            with pytest.raises(TypeError):
                dynamic_cls(0)
            with pytest.raises(TypeError):
                iter(dynamic_cls)
            for member in members.values():
                value: Any = member
                assert cls.__hash__ is None
                with pytest.raises(TypeError):
                    hash(value)
                assert type(int(value)) is int
                assert value == int(value)
                assert int(value) == value
                assert value == getattr(cls, value.__repr__().split(".")[-1])
                assert not hasattr(value, "value")
                assert not hasattr(value, "name")
                with pytest.raises(AttributeError):
                    setattr(value, next(iter(members)), value)
    assert len(checked) == 24
    assert "DirectionIntegers" in checked
