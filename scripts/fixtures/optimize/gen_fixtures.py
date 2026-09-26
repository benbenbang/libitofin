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


def extended_rosenbrock(x):
    """Adjacent-pair Rosenbrock chain with a 25-dimensional test instance."""
    return sum(100.0 * (x[i + 1] - x[i] * x[i]) ** 2 + (1.0 - x[i]) ** 2 for i in range(len(x) - 1))


def extended_rosenbrock_gradient(x):
    gradient = [0.0] * len(x)
    for i in range(len(x) - 1):
        residual = x[i + 1] - x[i] * x[i]
        gradient[i] += -400.0 * x[i] * residual - 2.0 * (1.0 - x[i])
        gradient[i + 1] += 200.0 * residual
    return gradient


LBFGSB = {
    "rosenbrock_box": {
        "objective": rosenbrock, "jac": rosenbrock_gradient,
        "x0": [-1.2, 1.0], "bounds": [[-2.0, 2.0]] * 2, "maxcor": 10,
    },
    "rosenbrock_face": {
        "objective": rosenbrock, "jac": rosenbrock_gradient,
        "x0": [-1.2, 1.0], "bounds": [[0.0, 0.5], [-2.0, 2.0]], "maxcor": 10,
    },
    "extended_rosenbrock25": {
        "objective": extended_rosenbrock, "jac": extended_rosenbrock_gradient,
        "x0": [-1.2 if i % 2 == 0 else 1.0 for i in range(25)],
        "bounds": [[-2.0, 2.0]] * 25, "maxcor": 5,
    },
}



def hs35(x):
    """Hock-Schittkowski 35, minimized at (4/3, 7/9, 4/9) with f = 1/9."""
    return 9 - 8 * x[0] - 6 * x[1] - 4 * x[2] + 2 * x[0] ** 2 + 2 * x[1] ** 2 + x[2] ** 2 + 2 * x[0] * x[1] + 2 * x[0] * x[2]


def hs71(x):
    """Hock-Schittkowski 71, minimized near (1, 4.743, 3.821, 1.379) with f = 17.014."""
    return x[0] * x[3] * (x[0] + x[1] + x[2]) + x[2]


def tutorial(x):
    """The distance to (1, 2.5), cut off by three half-planes."""
    return (x[0] - 1.0) ** 2 + (x[1] - 2.5) ** 2


def weighted(x):
    """A diagonal quadratic, paired with the plane x0 + x1 + x2 = 1."""
    return x[0] ** 2 + 2.0 * x[1] ** 2 + 3.0 * x[2] ** 2


SLSQP = {
    "hs35": {
        "objective": hs35,
        "constraints": [("ineq", lambda x: 3 - x[0] - x[1] - 2 * x[2])],
        "x0": [0.5, 0.5, 0.5],
        "bounds": [[0.0, None]] * 3,
    },
    "hs71": {
        "objective": hs71,
        "constraints": [
            ("ineq", lambda x: x[0] * x[1] * x[2] * x[3] - 25.0),
            ("eq", lambda x: sum(v * v for v in x) - 40.0),
        ],
        "x0": [1.0, 5.0, 5.0, 1.0],
        "bounds": [[1.0, 5.0]] * 4,
    },
    "tutorial": {
        "objective": tutorial,
        "constraints": [
            ("ineq", lambda x: x[0] - 2 * x[1] + 2),
            ("ineq", lambda x: -x[0] - 2 * x[1] + 6),
            ("ineq", lambda x: -x[0] + 2 * x[1] + 2),
        ],
        "x0": [2.0, 0.0],
        "bounds": [[0.0, None]] * 2,
    },
    "rosenbrock_disc": {
        "objective": rosenbrock,
        "constraints": [("ineq", lambda x: 1.5 - x[0] ** 2 - x[1] ** 2)],
        "x0": [-1.0, 0.5],
        "bounds": None,
    },
    "weighted_plane": {
        "objective": weighted,
        "constraints": [("eq", lambda x: x[0] + x[1] + x[2] - 1.0)],
        "x0": [1.0, 1.0, 1.0],
        "bounds": None,
    },
}


def central_gradient(function, x, h=1e-6):
    """The central-difference gradient, the same rule the Rust test applies."""
    gradient = []
    for i in range(len(x)):
        plus, minus = list(x), list(x)
        plus[i] += h
        minus[i] -= h
        gradient.append((function(plus) - function(minus)) / (2 * h))
    return gradient


def kkt_residual(case, x, multipliers):
    """The largest component of g - A' lambda, projected onto the bound cone at x."""
    residual = central_gradient(case["objective"], x)
    for (_, constraint), multiplier in zip(case["constraints"], multipliers):
        for i, component in enumerate(central_gradient(constraint, x)):
            residual[i] -= multiplier * component
    worst = 0.0
    for i, r in enumerate(residual):
        lower, upper = case["bounds"][i] if case["bounds"] else (None, None)
        if lower is not None and abs(x[i] - lower) <= 1e-10:
            r = min(r, 0.0)
        if upper is not None and abs(x[i] - upper) <= 1e-10:
            r = max(r, 0.0)
        worst = max(worst, abs(r))
    return worst


def solved_slsqp(definitions: dict) -> dict:
    """Run SciPy SLSQP over every case and record the optimum and its residuals.

    SciPy lists the multipliers equalities first; they are put back in the
    order the constraints are declared, which is the order the Rust solver uses.
    """
    from scipy.optimize import minimize

    results = {}
    for name, case in definitions.items():
        constraints = [{"type": kind, "fun": function} for kind, function in case["constraints"]]
        outcome = minimize(
            case["objective"], case["x0"], method="SLSQP", bounds=case["bounds"], constraints=constraints, options={"ftol": 1e-10, "maxiter": 200}
        )
        if not outcome.success:
            raise SystemExit(f"case {name} did not converge: {outcome.message}")
        x = outcome.x.tolist()
        order = [i for i, (kind, _) in enumerate(case["constraints"]) if kind == "eq"]
        order += [i for i, (kind, _) in enumerate(case["constraints"]) if kind == "ineq"]
        multipliers = [0.0] * len(order)
        for slot, index in enumerate(order):
            multipliers[index] = float(outcome.multipliers[slot])
        violation = max(
            [abs(f(x)) if kind == "eq" else max(0.0, -f(x)) for kind, f in case["constraints"]],
            default=0.0,
        )
        results[name] = {
            "x0": case["x0"],
            "bounds": case["bounds"],
            "ftol": 1e-10,
            "x": x,
            "fun": float(outcome.fun),
            "multipliers": multipliers,
            "max_violation": violation,
            "kkt_residual": kkt_residual(case, x, multipliers),
            "nit": int(outcome.nit),
            "message": str(outcome.message),
        }
    return results


CASES: dict[str, object] = {"nelder_mead": NELDER_MEAD, "bfgs": BFGS, "lbfgsb": LBFGSB, "slsqp": SLSQP}


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


def solved_lbfgsb(definitions: dict) -> dict:
    """Record objective quality, box feasibility and projected KKT residual."""
    from scipy.optimize import minimize

    results = {}
    for name, case in definitions.items():
        outcome = minimize(case["objective"], case["x0"], jac=case["jac"],
            method="L-BFGS-B", bounds=case["bounds"],
            options={"maxcor": case["maxcor"], "ftol": 1e-12, "gtol": 1e-5})
        if not outcome.success:
            raise SystemExit(f"case {name} did not converge: {outcome.message}")
        projected = []
        for value, component, (lower, upper) in zip(outcome.x, outcome.jac, case["bounds"]):
            if component > 0:
                projected.append(min(float(component), max(float(value - lower), 0.0)))
            else:
                projected.append(min(float(-component), max(float(upper - value), 0.0)))
        bounds = case["bounds"]
        compact_bounds = bounds[:1] if len(set(map(tuple, bounds))) == 1 else bounds
        results[name] = {
            "x0": case["x0"], "bounds": compact_bounds, "maxcor": case["maxcor"],
            "fun": float(outcome.fun), "projected_gnorm": max(projected),
            "status": int(outcome.status),
        }
    return results


SOLVERS = {"bfgs": solved_bfgs, "lbfgsb": solved_lbfgsb, "slsqp": solved_slsqp}


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
