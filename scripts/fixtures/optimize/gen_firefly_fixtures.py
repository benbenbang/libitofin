"""Independent seeded Firefly policy references from Yang arXiv:1003.1466v1.

Equations 6, 8 and 9 provide attraction, distance and uniform displacement.
Frozen ranking/targets, box-scaled distance/noise, reflection and geometric
alpha decay are explicit project policies, not a published trajectory promise.
Prepared from the approved contract before inspecting the Rust implementation.
"""

from __future__ import annotations

# standard library
import argparse
import copy
import hashlib
import json
import math
from decimal import Decimal, localcontext
from pathlib import Path

MASK = (1 << 64) - 1
DEFAULTS = {"seed": 0, "alpha": 0.25, "beta0": 1.0, "gamma": 1.0,
            "alpha_decay": 0.97, "xatol": 1e-6, "fatol": 1e-8}


class RandomStream:
    def __init__(self, seed):
        self.state = seed
        self.draws = 0

    def uniform(self):
        self.state = (self.state + 0x9E3779B97F4A7C15) & MASK
        word = self.state
        word = ((word ^ (word >> 30)) * 0xBF58476D1CE4E5B9) & MASK
        word = ((word ^ (word >> 27)) * 0x94D049BB133111EB) & MASK
        word ^= word >> 31
        self.draws += 1
        return (word >> 11) / 9007199254740992.0


def physical(bounds, unit):
    lower, upper = bounds
    width = upper - lower
    value = lower + unit * width if unit <= 0.5 else upper - (1.0 - unit) * width
    return min(upper, max(lower, value))


def objective(name, point):
    if name == "signed_quadratic":
        return (point[0] - 1.25) ** 2 + 4.0 * (point[1] + 0.75) ** 2 - 3.0
    if name == "quadratic":
        return point[0] ** 2
    if name == "boundary_quadratic":
        return (point[0] - 3.0) ** 2 - 20.0
    if name == "constant":
        return -7.0
    if name == "linear":
        return point[0]
    if name == "multimodal_polynomial":
        return (point[0] - 1.0) ** 2 * ((point[0] + 2.0) ** 2 + 0.1)
    if name == "himmelblau":
        return (point[0] ** 2 + point[1] - 11.0) ** 2 + (point[0] + point[1] ** 2 - 7.0) ** 2
    if name == "shifted_rastrigin":
        return 15.0 + sum(v * v - 10.0 * math.cos(2.0 * math.pi * v) for v in point)
    raise ValueError(name)


def coordinate_move(current, target, beta, noise, bounds):
    lower, upper = bounds
    width = upper - lower
    delta = target - current
    displacement = beta * delta + noise * width
    if displacement == 0.0:
        return current
    base = current + beta * delta if beta <= 0.5 else target - (1.0 - beta) * delta
    base = min(upper, max(lower, base))
    value = base + noise * width
    if math.isfinite(value) and lower <= value <= upper:
        return value
    fraction = (min(1.0, max(0.0, (base - lower) / width)) + noise) % 2.0
    fraction = fraction if fraction <= 1.0 else 2.0 - fraction
    return physical(bounds, min(1.0, max(0.0, fraction)))


def solve(case, trace=True):
    options = DEFAULTS | case.get("options", {})
    bounds = case["bounds"]
    free = [i for i, (lower, upper) in enumerate(bounds) if lower < upper]
    count = case.get("population_size", len(case.get("initial_population", [])) or max(8, 15 * len(free)))
    rng = RandomStream(options["seed"])
    best, first = None, None
    nfev, nit = 0, 0
    alpha = options["alpha"]
    evaluations, callbacks, generations = [], [], []
    maxfev, maxiter = case.get("maxfev", 1000000), case.get("maxiter", 1000)

    class Stop(Exception):
        pass

    def evaluate(point, stage, row, target=None):
        nonlocal nfev, best, first
        if nfev >= maxfev:
            raise Stop("max_evaluations")
        nfev += 1
        metadata = {"stage": stage, "row": row, "target": target}
        if case.get("fail_at_evaluation") == nfev:
            if trace:
                evaluations.append({"x": point[:], "fun": None, "error": True} | metadata)
            raise Stop("objective_error")
        value = math.nan if case.get("nonfinite_at_evaluation") == nfev else objective(case["objective"], point)
        item = {"x": point[:], "fun": value if math.isfinite(value) else None}
        if trace:
            evaluations.append(item | metadata)
        if first is None:
            first = item
        if not math.isfinite(value):
            raise Stop("nonfinite")
        if best is None or value < best["fun"]:
            best = item
        return value

    def result(status):
        winner = best or first or {"x": case["x0"][:], "fun": None}
        output = winner | {"nit": nit, "nfev": nfev, "njev": 0, "status": status,
                           "success": status == "converged"}
        if trace:
            output.update(evaluations=evaluations, callbacks=callbacks, generations=generations, rng_draws=rng.draws)
        return output

    def converged(population):
        values = [fly["fun"] for fly in population]
        if max(values) - min(values) > options["fatol"] or alpha > options["xatol"]:
            return False
        for index in free:
            positions = [fly["x"][index] for fly in population]
            spread = max(positions) - min(positions)
            if options["xatol"] == 0.0:
                if spread != 0.0:
                    return False
            elif spread / (bounds[index][1] - bounds[index][0]) > options["xatol"]:
                return False
        return True

    try:
        if not free:
            evaluate(case["x0"][:], "all_fixed", 0)
            return result("converged")
        population = []
        for row in range(count):
            if "initial_population" in case:
                point = case["initial_population"][row][:]
            elif row == 0:
                point = case["x0"][:]
            else:
                point = [lower for lower, _ in bounds]
                for index in free:
                    point[index] = physical(bounds[index], rng.uniform())
            value = evaluate(point, "initial", row)
            population.append({"x": point, "fun": value})
        while not converged(population):
            frozen = copy.deepcopy(population)
            updated = []
            for row, fly in enumerate(frozen):
                current, value = fly["x"][:], fly["fun"]
                targets = [j for j, other in enumerate(frozen) if other["fun"] < fly["fun"]]
                for target in targets or [None]:
                    destination = current if target is None else frozen[target]["x"]
                    r2 = sum(((destination[i] - current[i]) / (bounds[i][1] - bounds[i][0])) ** 2 for i in free)
                    beta = 0.0 if target is None else options["beta0"] * math.exp(-options["gamma"] * r2)
                    trial = current[:]
                    for index in free:
                        noise = alpha * (rng.uniform() - 0.5)
                        trial[index] = coordinate_move(current[index], destination[index], beta, noise, bounds[index])
                    if trial != current:
                        value = evaluate(trial, "random_walk" if target is None else "attraction", row, target)
                        current = trial
                updated.append({"x": current, "fun": value})
            population = updated
            alpha *= options["alpha_decay"]
            nit += 1
            state = {"x": best["x"][:], "fun": best["fun"], "nit": nit, "nfev": nfev, "njev": 0}
            if trace:
                callbacks.append(state)
                generations.append({"generation": nit, "alpha": alpha,
                                    "population": copy.deepcopy(population), "rng_draws": rng.draws})
            if case.get("fail_at_callback") == nit:
                raise Stop("callback_error")
            if case.get("cancel_after_generation") == nit:
                raise Stop("cancelled")
            if nit >= maxiter:
                raise Stop("max_iterations")
        return result("converged")
    except Stop as stopped:
        return result(str(stopped))


def cases():
    base = {"name": "signed_frozen_order_seed_zero", "objective": "signed_quadratic", "x0": [0.0, 0.0, 7.0],
            "bounds": [[-4.0, 4.0], [-3.0, 3.0], [7.0, 7.0]],
            "initial_population": [[3.0, 1.0, 7.0], [-2.0, -2.0, 7.0], [0.0, 2.0, 7.0], [1.0, -1.0, 7.0]],
            "maxiter": 2, "options": {"xatol": 0.0, "fatol": 0.0}}
    short = [base]
    for name, changes in [
        ("partial_initialization_budget", {"maxfev": 2}),
        ("partial_generation_archive", {"maxfev": 6}),
        ("completed_generation_budget", {"maxfev": 11}),
        ("callback_cancel_wins_iteration_budget", {"maxiter": 1, "cancel_after_generation": 1}),
        ("objective_error_on_attraction", {"fail_at_evaluation": 5}),
        ("callback_error", {"fail_at_callback": 1}),
        ("nonfinite_initial", {"nonfinite_at_evaluation": 1}),
        ("nonfinite_attraction_preserves_best", {"nonfinite_at_evaluation": 5}),
    ]:
        short.append(base | {"name": name} | changes)
    for name, options in [("zero_noise_attraction", {"alpha": 0.0}),
                          ("zero_absorption", {"gamma": 0.0}),
                          ("no_noise_decay", {"alpha_decay": 1.0}),
                          ("opaque_distance", {"gamma": 1e6})]:
        short.append(base | {"name": name, "options": base["options"] | options})
    random = base | {"name": "random_initialization_seed_zero", "population_size": 4}
    del random["initial_population"]
    short.append(random)
    short += [
        {"name": "alpha_zero_no_brighter_walk_consumes_rng_without_evaluation", "objective": "constant",
         "x0": [0.0], "bounds": [[-1.0, 1.0]], "initial_population": [[0.2], [-0.8], [0.8], [0.1]],
         "maxiter": 2, "maxfev": 4, "options": {"alpha": 0.0, "xatol": 0.0, "fatol": 0.0}},
        {"name": "hand_frozen_ranking_alpha_zero", "objective": "quadratic", "x0": [1.0],
         "bounds": [[-2.0, 2.0]], "initial_population": [[1.0], [0.0], [-2.0], [2.0]], "maxiter": 2,
         "options": {"alpha": 0.0, "gamma": 0.0, "xatol": 0.0, "fatol": 0.0}},
        {"name": "duplicate_initial_rows_evaluated_then_skip_moves", "objective": "constant", "x0": [0.0],
         "bounds": [[-1.0, 1.0]], "initial_population": [[0.2]] * 4, "maxiter": 3,
         "options": {"alpha": 0.0, "xatol": 0.0, "fatol": 0.0}},
        {"name": "ties_random_walk_keep_earliest_archive", "objective": "constant", "x0": [0.0],
         "bounds": [[-1.0, 1.0]], "initial_population": [[0.2], [-0.8], [0.8], [0.1]], "maxiter": 2},
        {"name": "noise_prevents_false_population_convergence", "objective": "constant", "x0": [0.0],
         "bounds": [[-1.0, 1.0]], "initial_population": [[0.2]] * 4, "maxiter": 2},
        {"name": "reflection_at_boundary", "objective": "boundary_quadratic", "x0": [1.0],
         "bounds": [[0.0, 1.0]], "initial_population": [[1.0], [0.99], [0.0], [0.01]], "maxiter": 2,
         "options": {"alpha": 1.0, "gamma": 0.0}},
        {"name": "huge_box_finite_noise", "objective": "constant", "x0": [0.0],
         "bounds": [[-1e308, 7e307]], "initial_population": [[0.0], [7e307], [-1e308], [1.0]], "maxiter": 2,
         "options": {"alpha": 1.0, "xatol": 0.0, "fatol": 0.0}},
        {"name": "microscopic_physical_distance_survives_normalization", "objective": "linear", "x0": [0.0],
         "bounds": [[-1e308, 7e307]], "initial_population": [[0.0], [1e-100], [-1e-100], [math.nextafter(0.0, 1.0)]],
         "maxiter": 2, "options": {"alpha": 0.0, "gamma": 0.0, "xatol": 0.0, "fatol": 0.0}},
        {"name": "all_fixed_x0_once_not_population", "objective": "signed_quadratic", "x0": [1.25, -0.75],
         "bounds": [[1.25, 1.25], [-0.75, -0.75]], "initial_population": [[1.25, -0.75]] * 4},
    ]
    quality = [
        {"name": "signed_analytic_quality", "objective": "signed_quadratic", "x0": [0.0, 0.0],
         "bounds": [[-4.0, 4.0], [-3.0, 3.0]], "analytic_x": [1.25, -0.75], "analytic_fun": -3.0},
        {"name": "multimodal_analytic_quality", "objective": "multimodal_polynomial", "x0": [-2.0],
         "bounds": [[-3.0, 3.0]], "analytic_x": [1.0], "analytic_fun": 0.0},
        {"name": "himmelblau_analytic_quality", "objective": "himmelblau", "x0": [0.0, 0.0],
         "bounds": [[-5.0, 5.0], [-5.0, 5.0]], "analytic_fun": 0.0, "multiple_minima": True},
        {"name": "rastrigin_analytic_quality", "objective": "shifted_rastrigin", "x0": [2.4, -1.7],
         "bounds": [[-5.12, 5.12], [-5.12, 5.12]], "analytic_x": [0.0, 0.0], "analytic_fun": -5.0},
    ]
    for case in quality:
        case.update(maxiter=1000, population_size=32, options={"seed": 42})
    quality[-1]["options"]["seed"] = 5
    quality.append(quality[-1] | {"name": "rastrigin_convergence_is_not_global",
                                 "options": {"seed": 42}, "global_gap": 0.013169176494838})
    return short, quality


def hand_attraction():
    with localcontext() as context:
        context.prec = 80
        beta = (-Decimal("0.5")).exp()
        movement = [float(beta - Decimal("0.125")), float(2 * beta - Decimal("0.25"))]
    return [{"current": [0.0, 0.0], "target": [1.0, 2.0], "bounds": [[0.0, 2.0], [0.0, 4.0]],
             "distance_squared": 0.5, "beta0": 1.0, "gamma": 1.0, "beta": float(beta),
             "alpha": 0.25, "uniforms": [0.25, 0.25], "expected_move": movement},
            {"distance_squared": 0.5, "beta0": 0.75, "gamma": 0.0, "beta": 0.75},
            {"distance_squared": 2.0, "beta0": 1.0, "gamma": 1e6, "beta": 0.0}]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path(__file__).resolve().parents[3]
                        / "crates/itofin-optimize/tests/fixtures/firefly.jsonl")
    args = parser.parse_args()
    short, quality = cases()
    records = [{"kind": "provenance", "value": {
        "implementation": "Independent standard-library equations from approved frozen-generation policy",
        "references": ["https://arxiv.org/html/1003.1466"],
        "selection": "Rastrigin seed5 reaches analytic optimum; seed42 converges with nonzero gap, both retained",
        "limitation": "Finite box-scaled heuristic; no global optimum or native trajectory guarantee"}}]
    records += [{"kind": "attraction", "value": row} for row in hand_attraction()]
    for case in short + quality:
        name = case["name"]
        traced = case in short
        expected = solve(case, traced)
        records.append({"kind": "input", "name": name, "value": case, "trace": traced})
        traces = {kind: expected.pop(key, []) for kind, key in
                  [("evaluation", "evaluations"), ("callback", "callbacks"), ("generation", "generations")]}
        expected.pop("rng_draws", None)
        records.append({"kind": "result", "name": name, "value": expected})
        for kind, rows in traces.items():
            records.extend({"kind": kind, "name": name, "value": row} for row in rows)
    args.output.write_text("\n".join(json.dumps(row, allow_nan=False) for row in records) + "\n")
    print(f"Generated {len(short)} policy cases and {len(quality)} analytic-quality cases")
    print(f"Fixture SHA256 {hashlib.sha256(args.output.read_bytes()).hexdigest()}")


if __name__ == "__main__":
    main()
