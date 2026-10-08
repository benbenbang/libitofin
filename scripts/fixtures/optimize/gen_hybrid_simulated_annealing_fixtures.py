"""Independent standard-library hybrid annealing policy and analytic references.

Derived from the approved contract and Kirkpatrick/Gelatt/Vecchi (1983),
DOI 10.1126/science.220.4598.671, before inspecting the Rust implementation.
Geometric reheating and bounded coordinate polling are project policies, not
Hajek's logarithmic asymptotic schedule or a QuantLib/SciPy trajectory promise.
"""

from __future__ import annotations

# standard library
import argparse
import hashlib
import json
import math
from decimal import Decimal, localcontext
from pathlib import Path

MASK = (1 << 64) - 1
DEFAULTS = {
    "seed": 0,
    "initial_temperature": 1.0,
    "cooling_rate": 0.95,
    "step_size": 0.25,
    "local_search_interval": 10,
    "local_search_steps": 4,
    "reanneal_interval": 100,
    "xatol": 1e-6,
    "fatol": 1e-8,
}


class RandomStream:
    def __init__(self, seed):
        self.state = seed
        self.draws = 0

    def uniform(self):
        self.state = (self.state + 0x9E3779B97F4A7C15) & MASK
        bits = self.state
        bits = ((bits ^ (bits >> 30)) * 0xBF58476D1CE4E5B9) & MASK
        bits = ((bits ^ (bits >> 27)) * 0x94D049BB133111EB) & MASK
        bits ^= bits >> 31
        self.draws += 1
        return (bits >> 11) / 9007199254740992.0


def physical(bounds, unit):
    lower, upper = bounds
    width = upper - lower
    value = lower + unit * width if unit <= 0.5 else upper - (1.0 - unit) * width
    return min(upper, max(lower, value))


def acceptance(current, trial, temperature):
    if trial <= current:
        return 1.0
    delta = trial - current
    scaled = delta / temperature if math.isfinite(delta) else trial / temperature - current / temperature
    return math.exp(-scaled)


def objective(name, point):
    if name == "signed_quadratic":
        return (point[0] - 1.25) ** 2 + 4.0 * (point[1] + 0.75) ** 2 - 3.0
    if name == "boundary_quadratic":
        return (point[0] - 3.0) ** 2 - 20.0
    if name in ("constant", "rounded_normalization"):
        return -7.0
    if name == "multimodal_polynomial":
        return (point[0] - 1.0) ** 2 * ((point[0] + 2.0) ** 2 + 0.1)
    if name == "himmelblau":
        return (point[0] * point[0] + point[1] - 11.0) ** 2 + (point[0] + point[1] * point[1] - 7.0) ** 2
    if name == "shifted_rastrigin":
        return 15.0 + sum(v * v - 10.0 * math.cos(2.0 * math.pi * v) for v in point)
    raise ValueError(name)


def solve(case, trace=True):
    config = DEFAULTS | case.get("options", {})
    free = [i for i, (lower, upper) in enumerate(case["bounds"]) if lower < upper]
    rng = RandomStream(config["seed"])
    evaluations, callbacks, cycles = [], [], []
    best = None
    first = None
    nfev, nit = 0, 0
    temperature = config["initial_temperature"]
    radius = config["step_size"]
    maxfev, maxiter = case.get("maxfev", 1000000), case.get("maxiter", 1000)

    class Stop(Exception):
        pass

    def evaluate(point, stage):
        nonlocal nfev, best, first
        if nfev >= maxfev:
            raise Stop("max_evaluations")
        nfev += 1
        if case.get("fail_at_evaluation") == nfev:
            if trace:
                evaluations.append({"x": point[:], "fun": None, "stage": stage, "error": True})
            raise Stop("objective_error")
        value = math.nan if case.get("nonfinite_at_evaluation") == nfev else objective(case["objective"], point)
        item = {"x": point[:], "fun": value if math.isfinite(value) else None}
        if trace:
            evaluations.append(item | {"stage": stage})
        if first is None:
            first = item
        if not math.isfinite(value):
            raise Stop("nonfinite")
        if best is None or value < best["fun"]:
            best = item
        return value

    def result(status):
        winner = best or first or {"x": case["x0"], "fun": None}
        output = winner | {"nit": nit, "nfev": nfev, "njev": 0, "status": status,
                           "success": status == "converged"}
        if trace:
            output.update(evaluations=evaluations, callbacks=callbacks, cycles=cycles, rng_draws=rng.draws)
        return output

    try:
        current = case["x0"][:]
        current_value = evaluate(current, "initial")
        if not free:
            return result("converged")
        while True:
            trial = current[:]
            for index in free:
                lower, upper = case["bounds"][index]
                delta = (2.0 * rng.uniform() - 1.0) * config["step_size"]
                if delta == 0.0:
                    continue
                unit = (current[index] - lower) / (upper - lower)
                unit += delta
                unit = -unit if unit < 0.0 else 2.0 - unit if unit > 1.0 else unit
                trial[index] = physical(case["bounds"][index], unit)
            trial_value = current_value if trial == current else evaluate(trial, "proposal")
            accepted = trial_value <= current_value
            if not accepted:
                accepted = rng.uniform() < acceptance(current_value, trial_value, temperature)
            if accepted:
                current, current_value = trial, trial_value
            pending = False
            if (nit + 1) % config["local_search_interval"] == 0:
                for sweep in range(config["local_search_steps"]):
                    start_value = best["fun"]
                    observed = [start_value]
                    improved = False
                    for index in free:
                        for direction in (1.0, -1.0):
                            anchor = best["x"]
                            candidate = anchor[:]
                            lower, upper = case["bounds"][index]
                            unit = (anchor[index] - lower) / (upper - lower)
                            candidate[index] = physical(case["bounds"][index], min(1.0, max(0.0, unit + direction * radius)))
                            if candidate == anchor:
                                continue
                            incumbent = best["fun"]
                            value = evaluate(candidate, "local")
                            observed.append(value)
                            if value < incumbent:
                                improved = True
                                current, current_value = best["x"][:], best["fun"]
                    if not improved:
                        pending = radius <= config["xatol"] and max(observed) - min(observed) <= config["fatol"]
                        if pending:
                            break
                        radius *= 0.5
                current, current_value = best["x"][:], best["fun"]
            nit += 1
            callback = {"x": best["x"][:], "fun": best["fun"], "nit": nit, "nfev": nfev, "njev": 0}
            if trace:
                callbacks.append(callback)
                cycles.append({"cycle": nit, "temperature": temperature, "radius": radius,
                               "accepted": accepted, "chain": current[:], "chain_fun": current_value,
                               "rng_draws": rng.draws})
            if case.get("fail_at_callback") == nit:
                raise Stop("callback_error")
            if case.get("cancel_after_cycle") == nit:
                raise Stop("cancelled")
            if nit >= maxiter:
                raise Stop("max_iterations")
            if pending:
                raise Stop("converged")
            temperature = max(math.nextafter(0.0, 1.0), temperature * config["cooling_rate"])
            if nit % config["reanneal_interval"] == 0:
                temperature = config["initial_temperature"]
                current, current_value = best["x"][:], best["fun"]
    except Stop as stopped:
        return result(str(stopped))


def cases():
    base = {"name": "signed_reflection_and_polish", "objective": "signed_quadratic", "x0": [3.5, 2.5, 7.0],
            "bounds": [[-4.0, 4.0], [-3.0, 3.0], [7.0, 7.0]], "maxiter": 4,
            "options": {"seed": 0, "local_search_interval": 2, "local_search_steps": 2,
                        "reanneal_interval": 3, "xatol": 0.0, "fatol": 0.0}}
    short = [base]
    for name, overrides in [
        ("initial_only_budget", {"maxfev": 1}),
        ("partial_local_poll_budget", {"maxfev": 4}),
        ("completed_cycle_budget", {"maxfev": 2}),
        ("callback_cancel_wins_iteration_limit", {"maxiter": 1, "cancel_after_cycle": 1}),
        ("callback_error", {"fail_at_callback": 1}),
        ("objective_error_in_local_poll", {"fail_at_evaluation": 4}),
        ("nonfinite_initial", {"nonfinite_at_evaluation": 1}),
        ("nonfinite_local_keeps_prior_best", {"nonfinite_at_evaluation": 4}),
    ]:
        short.append(base | {"name": name} | overrides)
    short += [
        {"name": "rounded_proposals_are_not_charged", "objective": "constant", "x0": [1.0],
         "bounds": [[1.0, math.nextafter(1.0, math.inf)]], "maxiter": 3,
         "options": {"step_size": 1e-300, "local_search_interval": 1, "xatol": 0.0}},
        {"name": "temperature_underflow_floor", "objective": "constant", "x0": [0.2],
         "bounds": [[-1.0, 1.0]], "maxiter": 3,
         "options": {"initial_temperature": math.nextafter(0.0, 1.0), "cooling_rate": 0.5}},
        {"name": "boundary_local_clipping_skips_duplicates", "objective": "boundary_quadratic", "x0": [1.0],
         "bounds": [[0.0, 1.0]], "maxiter": 3,
         "options": {"step_size": 1.0, "local_search_interval": 1, "local_search_steps": 2, "reanneal_interval": 2}},
        {"name": "ties_preserve_first", "objective": "constant", "x0": [0.2], "bounds": [[-1.0, 1.0]],
         "maxiter": 3, "options": {"local_search_interval": 1, "local_search_steps": 2, "xatol": 0.0}},
        {"name": "constant_local_convergence", "objective": "constant", "x0": [0.2], "bounds": [[-1.0, 1.0]],
         "maxiter": 3, "options": {"local_search_interval": 1, "local_search_steps": 2, "xatol": 0.25}},
        {"name": "convergence_loses_to_iteration_limit", "objective": "constant", "x0": [0.2],
         "bounds": [[-1.0, 1.0]], "maxiter": 1, "options": {"local_search_interval": 1, "xatol": 0.25}},
        {"name": "reanneal_does_not_restore_radius", "objective": "constant", "x0": [0.2],
         "bounds": [[-1.0, 1.0]], "maxiter": 4,
         "options": {"local_search_interval": 1, "local_search_steps": 1, "reanneal_interval": 2, "xatol": 0.0}},
        {"name": "huge_finite_box_stays_finite", "objective": "rounded_normalization", "x0": [0.0],
         "bounds": [[-1e308, 7e307]], "maxiter": 3,
         "options": {"local_search_interval": 1, "local_search_steps": 2, "xatol": 0.0}},
        {"name": "all_fixed_once", "objective": "signed_quadratic", "x0": [1.25, -0.75],
         "bounds": [[1.25, 1.25], [-0.75, -0.75]]},
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
        case.update(maxiter=1000, options={"seed": 42, "initial_temperature": 20.0})
    quality[-1]["options"]["seed"] = 1
    limited = quality[-1] | {"name": "rastrigin_local_minimum_is_not_global",
                            "options": quality[-1]["options"] | {"seed": 42},
                            "global_gap": 0.994959058809727}
    quality.append(limited)
    return short, quality


def hand_acceptance():
    rows = []
    for current, trial, temperature in [(-2.0, -3.0, 1.0), (-2.0, -2.0, 1.0), (-2.0, -1.0, 1.0),
                                         (0.0, 2.0, 2.0), (-1e308, 1e308, 1e308), (-1e308, 1e308, 1.0)]:
        with localcontext() as context:
            context.prec = 80
            scaled = (Decimal(trial) - Decimal(current)) / Decimal(temperature)
            probability = Decimal(1) if scaled <= 0 else (-scaled).exp() if scaled < 10000 else Decimal(0)
        expected = float(probability)
        actual = acceptance(current, trial, temperature)
        assert math.isclose(actual, expected, rel_tol=1e-14, abs_tol=0.0)
        rows.append({"current": current, "trial": trial, "temperature": temperature, "probability": expected})
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path(__file__).resolve().parents[3]
                        / "crates/itofin-optimize/tests/fixtures/hybrid_simulated_annealing.jsonl")
    args = parser.parse_args()
    short, quality = cases()
    provenance = {"implementation": "Independent standard-library equations from approved policy",
                  "references": ["https://doi.org/10.1126/science.220.4598.671",
                                 "https://doi.org/10.1287/moor.13.2.311"],
                  "limitation": "Finite-budget geometric reheating hybrid, no global optimum guarantee",
                  "selection": "Rastrigin seed1 reaches known optimum; seed42 retains local optimum limitation",
                  "format": "JSONL: one auditable input, result, evaluation, callback or cycle per record"}
    records = [{"kind": "provenance", "value": provenance}]
    records += [{"kind": "acceptance", "value": row} for row in hand_acceptance()]
    for case in short + quality:
        traced = case in short
        expected = solve(case, trace=traced)
        name = case["name"]
        records.append({"kind": "input", "name": name, "value": case, "trace": traced})
        traces = {kind: expected.pop(key, []) for kind, key in
                  [("evaluation", "evaluations"), ("callback", "callbacks"), ("cycle", "cycles")]}
        expected.pop("rng_draws", None)
        records.append({"kind": "result", "name": name, "value": expected})
        for kind, rows in traces.items():
            records.extend({"kind": kind, "name": name, "value": row} for row in rows)
    args.output.write_text("\n".join(json.dumps(row, allow_nan=False) for row in records) + "\n")
    print(f"Generated {len(short)} policy cases and {len(quality)} analytic-quality cases")
    print(f"Fixture SHA256 {hashlib.sha256(args.output.read_bytes()).hexdigest()}")


if __name__ == "__main__":
    main()
