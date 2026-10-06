"""Generate development-only SciPy and analytic differential-evolution fixtures."""

import argparse
import datetime
import json
import math
from pathlib import Path


def value(name, x):
    if name == "negative_quadratic":
        return (x[0] - 1.25) ** 2 + 4 * (x[1] + 0.75) ** 2 - 3
    if name == "multimodal_polynomial":
        return (x[0] - 1) ** 2 * ((x[0] + 2) ** 2 + 0.1)
    if name == "shifted_rastrigin":
        return 20 + sum(v * v - 10 * math.cos(2 * math.pi * v) for v in x) - 5
    if name == "boundary_quadratic":
        return (x[0] - 3) ** 2 + (x[1] + 4) ** 2 - 20
    if name == "fixed_quadratic":
        return (x[0] - 2) ** 2 + (x[1] - 0.375) ** 2 - 7
    raise ValueError(name)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path(__file__).resolve().parents[3]
        / "crates/itofin-optimize/tests/fixtures/global.json",
    )
    args = parser.parse_args()
    import scipy
    from scipy.optimize import differential_evolution

    specs = [
        (
            "negative_quadratic",
            [-4, 4],
            [[-5, 5], [-5, 5]],
            [1.25, -0.75],
            -3,
            1e-5,
            1e-8,
        ),
        ("multimodal_polynomial", [-1.966], [[-3, 2]], [1], 0, 1e-5, 1e-8),
        ("shifted_rastrigin", [4, -4], [[-5.12, 5.12]] * 2, [0, 0], -5, 1e-5, 1e-7),
        ("boundary_quadratic", [0, 0], [[-1, 1]] * 2, [1, -1], -7, 1e-5, 1e-4),
        ("fixed_quadratic", [2, -2], [[2, 2], [-3, 3]], [2, 0.375], -7, 1e-5, 1e-8),
    ]
    cases = []
    for name, x0, bounds, optimum, fun, xtol, ftol in specs:
        result = differential_evolution(
            lambda x: value(name, x),
            bounds,
            strategy="rand1bin",
            mutation=0.8,
            recombination=0.9,
            updating="deferred",
            polish=False,
            rng=42,
            popsize=32,
            maxiter=2000,
            atol=1e-12,
            tol=1e-12,
            x0=x0,
        )
        if not result.success or abs(result.fun - fun) > ftol:
            raise RuntimeError(f"independent comparison failed: {name}: {result}")
        cases.append(
            dict(
                name=name,
                x0=x0,
                bounds=bounds,
                analytic_x=optimum,
                analytic_fun=fun,
                point_tolerance=xtol,
                value_tolerance=ftol,
                scipy_x=result.x.tolist(),
                scipy_fun=float(result.fun),
                scipy_nfev=int(result.nfev),
                scipy_nit=int(result.nit),
            )
        )
    data = dict(
        provenance=dict(
            scipy_version=scipy.__version__,
            generated=datetime.date.today().isoformat(),
            strategy="rand1bin",
            updating="deferred",
            polish=False,
            rng=42,
            popsize_multiplier=32,
            maxiter=2000,
            atol=1e-12,
            tol=1e-12,
            comparison="Analytic optimum and objective quality, not trajectories or seeds",
            primary_reference="https://doi.org/10.1023/A:1008202821328",
        ),
        cases=cases,
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
