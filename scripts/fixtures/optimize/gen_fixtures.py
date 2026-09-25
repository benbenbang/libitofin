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


def rosenbrock_gradient(x):
    """The analytic gradient of `rosenbrock`."""
    r = x[1] - x[0] * x[0]
    return [-400.0 * x[0] * r - 2.0 * (1.0 - x[0]), 200.0 * r]


def powell_singular(x):
    """Powell's singular function, minimized at the origin with a singular Hessian there."""
    return (
        (x[0] + 10.0 * x[1]) ** 2
        + 5.0 * (x[2] - x[3]) ** 2
        + (x[1] - 2.0 * x[2]) ** 4
        + 10.0 * (x[0] - x[3]) ** 4
    )


def powell_singular_gradient(x):
    """The analytic gradient of `powell_singular`."""
    a, b, c, d = x[0] + 10.0 * x[1], x[2] - x[3], x[1] - 2.0 * x[2], x[0] - x[3]
    return [
        2.0 * a + 40.0 * d**3,
        20.0 * a + 4.0 * c**3,
        10.0 * b - 8.0 * c**3,
        -10.0 * b - 40.0 * d**3,
    ]


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

BFGS = {
    "rosenbrock": {"objective": rosenbrock, "jac": rosenbrock_gradient, "x0": [-1.2, 1.0]},
    "beale": {"objective": beale, "jac": None, "x0": [1.0, 1.0]},
    "powell_singular": {
        "objective": powell_singular,
        "jac": powell_singular_gradient,
        "x0": [3.0, -1.0, 0.0, 1.0],
    },
}

CASES: dict[str, object] = {"nelder_mead": NELDER_MEAD, "bfgs": BFGS}


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


def solved_bfgs(definitions: dict) -> dict:
    """Run SciPy BFGS with its default `gtol` over every case.

    A case without `jac` uses SciPy's forward differences. The gradient norm
    at the optimum is recorded so the residual has a named oracle.
    """
    from scipy.optimize import minimize

    results = {}
    for name, case in definitions.items():
        outcome = minimize(case["objective"], case["x0"], jac=case["jac"], method="BFGS")
        if not outcome.success:
            raise SystemExit(f"case {name} did not converge: {outcome.message}")
        results[name] = {
            "x0": case["x0"],
            "analytic_gradient": case["jac"] is not None,
            "x": outcome.x.tolist(),
            "fun": float(outcome.fun),
            "gnorm": float(abs(outcome.jac).max()),
            "nit": int(outcome.nit),
            "nfev": int(outcome.nfev),
            "njev": int(outcome.njev),
            "status": int(outcome.status),
            "message": str(outcome.message),
        }
    return results


SOLVERS = {"bfgs": solved_bfgs}


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
        solve = SOLVERS.get(name, solved)
        print(write_fixture(args.output, name, solve(definitions), scipy.__version__))


if __name__ == "__main__":
    main()
