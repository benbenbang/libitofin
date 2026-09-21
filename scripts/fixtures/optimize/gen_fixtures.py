"""Generate the SciPy reference fixtures for the itofin-optimize test suite.

Every file written records the SciPy version and the generation date, so a
fixture that drifts against a later SciPy release is identifiable from the
fixture alone. SciPy is imported lazily, inside `main`, so that `--help` works
on a machine without it.

Solver tickets register their own cases in `CASES`; none exist yet.
"""

import argparse
import datetime
import json
import pathlib

DEFAULT_OUTPUT = pathlib.Path(__file__).resolve().parents[3] / "crates/itofin-optimize/tests/fixtures"

CASES: dict[str, object] = {}


def provenance(scipy_version: str) -> dict[str, str]:
    """Stamp every fixture with the SciPy release and day that produced it."""
    return {
        "scipy_version": scipy_version,
        "generated": datetime.date.today().isoformat(),
    }


def write_fixture(output: pathlib.Path, name: str, cases: object, scipy_version: str) -> pathlib.Path:
    """Write one fixture file and return where it landed."""
    output.mkdir(parents=True, exist_ok=True)
    path = output / f"{name}.json"
    payload = {"provenance": provenance(scipy_version), "cases": cases}
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
    return path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=pathlib.Path, default=DEFAULT_OUTPUT, help="where to write the fixture files")
    args = parser.parse_args()
    if not CASES:
        parser.exit(0, "no fixture cases are registered yet: each solver ticket adds its own\n")
    import scipy

    for name, cases in CASES.items():
        print(write_fixture(args.output, name, cases, scipy.__version__))


if __name__ == "__main__":
    main()
