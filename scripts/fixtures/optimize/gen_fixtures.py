"""Generate the SciPy reference fixtures for the itofin-optimize test suite.

Every file written records the SciPy version and the generation date, so a
fixture that drifts against a later SciPy release is identifiable from the
fixture alone. SciPy is imported lazily, inside `main`, so that `--help` works
on a machine without it.

Solver tickets register their own cases in `CASES`, keyed by the fixture file
they land in. A case names the objective, the starting point and the options;
`solved` runs SciPy over them and records where it arrived.
"""

import argparse
import datetime
import json
import pathlib

DEFAULT_OUTPUT = pathlib.Path(__file__).resolve().parents[3] / "crates/itofin-optimize/tests/fixtures"


def rosenbrock(x):
    """The banana valley, minimized at (1, 1)."""
    return 100.0 * (x[1] - x[0] * x[0]) ** 2 + (1.0 - x[0]) ** 2


def sphere(x):
    """The sum of squares, minimized at the origin."""
    return float(sum(value * value for value in x))


def beale(x):
    """Beale's function, minimized at (3, 0.5)."""
    return (
        (1.5 - x[0] + x[0] * x[1]) ** 2
        + (2.25 - x[0] + x[0] * x[1] * x[1]) ** 2
        + (2.625 - x[0] + x[0] * x[1] ** 3) ** 2
    )


def quadratic(x):
    """A positive definite quadratic with cross terms, minimized at (1, -2, 0.5, -1)."""
    a, b, c, d = x[0] - 1.0, x[1] + 2.0, x[2] - 0.5, x[3] + 1.0
    return a * a + 2.0 * b * b + 3.0 * c * c + 4.0 * d * d + 0.5 * a * b + 0.25 * b * c + 0.1 * c * d


TIGHT = {"xatol": 1e-08, "fatol": 1e-10, "maxiter": 20000, "maxfev": 20000}

NELDER_MEAD = {
    "rosenbrock": {"objective": rosenbrock, "x0": [-1.2, 1.0], "options": TIGHT},
    "sphere5": {"objective": sphere, "x0": [1.0, -2.0, 3.0, 0.5, -1.5], "options": TIGHT},
    "beale": {"objective": beale, "x0": [1.0, 1.0], "options": TIGHT},
    "quadratic4": {"objective": quadratic, "x0": [0.0, 0.0, 0.0, 0.0], "options": TIGHT},
    "adaptive_sphere10": {
        "objective": sphere,
        "x0": [0.6] * 10,
        "options": dict(TIGHT, adaptive=True),
    },
}

CASES: dict[str, object] = {"nelder_mead": NELDER_MEAD}


def solved(definitions: dict) -> dict:
    """Run SciPy over every case and record the point and the counts it reached.

    A case that does not converge is a broken fixture, not a recorded failure,
    so it stops the generator instead of reaching the JSON.
    """
    from scipy.optimize import minimize

    results = {}
    for name, case in definitions.items():
        outcome = minimize(
            case["objective"],
            case["x0"],
            method="Nelder-Mead",
            options=case["options"],
        )
        if not outcome.success:
            raise SystemExit(f"case {name} did not converge: {outcome.message}")
        results[name] = {
            "x0": case["x0"],
            "options": case["options"],
            "x": outcome.x.tolist(),
            "fun": float(outcome.fun),
            "nit": int(outcome.nit),
            "nfev": int(outcome.nfev),
            "status": int(outcome.status),
            "message": str(outcome.message),
        }
    return results


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

    for name, definitions in CASES.items():
        print(write_fixture(args.output, name, solved(definitions), scipy.__version__))


if __name__ == "__main__":
    main()
