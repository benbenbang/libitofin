#!/usr/bin/env python3
"""Audit declared Python symbols against reviewed C/Go mapping manifests.

This is API surface coverage, not execution/line coverage. Overloads count once;
inherited methods and imported modules do not add symbols. A mapped method also
establishes its containing type. Test references are checked for existence, not
treated as proof that every argument combination or code path was exercised.
"""
import argparse
import ast
import json
import re
import subprocess
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASELINE = "bf6c5d640c1a0aac3184d8a24d77897e2df2b5ae"


def inventory(ref=None):
    symbols = {}
    python_root = Path("crates/itofin-py/python")
    if ref is None:
        sources = ((p.relative_to(ROOT), p.read_text()) for p in sorted((ROOT / python_root / "itofin").rglob("*.pyi")))
    else:
        paths = subprocess.check_output(
            ["git", "ls-tree", "-r", "--name-only", "-z", ref, "--", str(python_root / "itofin")],
            cwd=ROOT, text=True,
        ).split("\0")
        sources = (
            (Path(p), subprocess.check_output(["git", "show", f"{ref}:{p}"], cwd=ROOT, text=True))
            for p in paths if p.endswith(".pyi")
        )
    for path, source in sources:
        module = ".".join(path.relative_to(python_root).parts[:-1])
        for node in ast.parse(source).body:
            if isinstance(node, ast.ClassDef):
                name = f"{module}.{node.name}"
                symbols[name] = "type"
                for child in node.body:
                    if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef)):
                        symbols[f"{name}.{child.name}"] = "method"
                    elif isinstance(child, (ast.Assign, ast.AnnAssign)):
                        targets = child.targets if isinstance(child, ast.Assign) else [child.target]
                        for target in targets:
                            if isinstance(target, ast.Name) and not target.id.startswith("_"):
                                symbols[f"{name}.{target.id}"] = "member"
            elif isinstance(node, ast.FunctionDef):
                symbols[f"{module}.{node.name}"] = "function"
            elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
                if node.target.id == "__version__":
                    symbols[f"{module}.__version__"] = "attribute"
    return symbols


def items(value):
    return [value] if isinstance(value, str) else value or []


def audit():
    symbols = inventory()
    records, errors = {}, []
    native = "\n".join(p.read_text() for p in (ROOT / "crates/libitofin-ffi/src").glob("*.rs"))
    header = (ROOT / "crates/libitofin-ffi/include/itofin.h").read_text()
    go = "\n".join(p.read_text() for p in (ROOT / "sdk/go").glob("*.go") if not p.name.endswith("_test.go"))
    tests = native + "\n" + "\n".join(p.read_text() for p in (ROOT / "sdk/go").glob("*_test.go"))
    for path in sorted((ROOT / "docs/go-coverage").glob("*.json")):
        data = json.loads(path.read_text())
        for record in data.get("entries", data.get("symbols", [])):
            name = record["python"]
            if not name.startswith("itofin."):
                matches = [s for s in symbols if s.endswith("." + name)]
                if len(matches) == 1:
                    name = matches[0]
            if name not in symbols:
                errors.append(f"{path.name}: unknown Python symbol {name}")
                continue
            if record.get("status") != "implemented":
                continue
            if not record.get("go"):
                errors.append(f"{name}: missing Go mapping")
            for value in items(record.get("c")):
                for export in re.findall(r"\bitofin_[a-z_0-9]+\b", value):
                    if not re.search(r"\bfn\s+" + export + r"\s*\(", native) or export not in header:
                        errors.append(f"{name}: missing C export {export}")
            for value in items(record.get("go")):
                # Descriptive suffixes explain operators and field/config mappings.
                ident = re.split(r"[: (]", value)[0].split(".")[-1]
                if ident and not re.search(r"\b" + re.escape(ident) + r"\b", go):
                    errors.append(f"{name}: missing Go identifier {ident}")
            for value in items(record.get("tests")):
                ident = value.split("::")[-1]
                if not re.search(r"\b(?:func|fn)\s+" + re.escape(ident) + r"\s*\(", tests):
                    errors.append(f"{name}: missing test {value}")
            records.setdefault(name, []).append(record)
    implied_types = {
        name.rsplit(".", 1)[0] for name in records
        if symbols.get(name.rsplit(".", 1)[0]) == "type"
    }
    covered = set(records) | implied_types
    linked_tests = {name for name, rows in records.items() if any(r.get("tests") for r in rows)}
    return symbols, covered, linked_tests, records, errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--strict", action="store_true", help="fail on any unmapped Python symbol")
    parser.add_argument("--baseline", action="store_true", help=f"require parity with {BASELINE}; report newer gaps separately")
    parser.add_argument("--report", type=Path, help="write JSON audit report")
    args = parser.parse_args()
    symbols, covered, linked, records, errors = audit()
    missing = sorted(set(symbols) - covered)
    required = set(symbols)
    if args.baseline:
        try:
            required = set(inventory(BASELINE))
        except subprocess.CalledProcessError:
            parser.error(f"cannot read baseline {BASELINE}; fetch repository history first")
        if not required:
            parser.error(f"baseline {BASELINE} contains no Python API symbols")
    required_missing = sorted(required - covered)
    newer_missing = sorted(set(missing) - required)
    report = {
        "metric": "declared Python API symbols; overloads deduplicated; containing types inferred from mapped members",
        "total": len(symbols), "mapped": len(covered), "with_test_references": len(linked),
        "kinds": dict(Counter(symbols.values())), "missing": missing, "errors": errors,
        "mappings": {s: records.get(s, [{"note": "type established by mapped member"}]) for s in sorted(covered)},
    }
    if args.baseline:
        report["baseline"] = {
            "revision": BASELINE, "total": len(required), "mapped": len(required & covered),
            "missing": required_missing, "newer_unmapped": newer_missing,
        }
    if args.report:
        args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(f"Python API mappings: {len(covered)}/{len(symbols)}; {len(linked)} explicitly link tests")
    if args.baseline:
        print(f"Baseline API mappings ({BASELINE}): {len(required & covered)}/{len(required)}")
    for error in errors:
        print("ERROR:", error)
    for name in required_missing:
        print("UNMAPPED:", name)
    for name in newer_missing:
        print("NEW UNMAPPED (outside baseline):", name)
    return bool(errors or (args.strict and required_missing))


if __name__ == "__main__":
    raise SystemExit(main())
