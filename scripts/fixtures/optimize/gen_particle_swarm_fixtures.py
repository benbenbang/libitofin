"""Independent synchronous PSO references from Kennedy/Eberhart 1995 and Shi/Eberhart 1998.

Standard-library equations written before inspecting the production PSO solver.
This is our explicit seeded policy, not a native trajectory compatibility claim.
"""

from __future__ import annotations

import copy
import json
import argparse
import math
from pathlib import Path

MASK = (1 << 64) - 1


class SplitMix64:
    def __init__(self, seed):
        self.state = seed
        self.draws = 0

    def unit(self):
        self.state = (self.state + 0x9E3779B97F4A7C15) & MASK
        word = self.state
        word = ((word ^ (word >> 30)) * 0xBF58476D1CE4E5B9) & MASK
        word = ((word ^ (word >> 27)) * 0x94D049BB133111EB) & MASK
        word ^= word >> 31
        self.draws += 1
        return (word >> 11) * (1.0 / 9007199254740992.0)


def coordinate(bounds, index, unit):
    lower, upper = bounds[index]
    width = upper - lower
    result = lower + unit * width if unit <= 0.5 else upper - (1.0 - unit) * width
    return min(upper, max(lower, result))


def objective(name, point):
    if name == 'signed_quadratic':
        return (point[0] - 1.25) ** 2 + 4.0 * (point[1] + 0.75) ** 2 - 3.0
    if name == 'boundary_quadratic':
        return (point[0] - 3.0) ** 2 - 20.0
    if name in ('constant', 'rounded_normalization'):
        return -7.0
    if name == 'multimodal_polynomial':
        return (point[0] - 1.0) ** 2 * ((point[0] + 2.0) ** 2 + 0.1)
    if name == 'himmelblau':
        return (point[0] * point[0] + point[1] - 11.0) ** 2 + (
            point[0] + point[1] * point[1] - 7.0
        ) ** 2
    if name == 'shifted_rastrigin':
        return 15.0 + sum(
            v * v - 10.0 * math.cos(2.0 * math.pi * v) for v in point
        )
    raise ValueError(name)


def run(case, full_trace=True):
    bounds = case['bounds']
    free = [i for i, (lower, upper) in enumerate(bounds) if lower < upper]
    rng = SplitMix64(case.get('seed', 0))
    options = {'inertia': 0.7, 'cognitive': 1.4, 'social': 1.4, 'velocity_clamp': 0.2}
    options.update(case.get('coefficients', {}))
    xatol = case.get('xatol', 1e-6)
    fatol = case.get('fatol', 1e-8)
    maxiter = case.get('maxiter', 1000)
    maxfev = case.get('maxfev', 1000000)
    size = case.get('population_size', len(case.get('initial_population', [])) or max(8, 15 * len(free)))
    evaluations = []
    callbacks = []
    snapshots = []
    best = None
    nfev = 0
    nit = 0

    def result(status):
        output = {
            'x': best['x'] if best else case['x0'],
            'fun': best['fun'] if best else None,
            'nit': nit,
            'nfev': nfev,
            'njev': 0,
            'status': status,
            'success': status == 'converged',
            'rng_draws': rng.draws,
            'callbacks': callbacks,
        }
        if full_trace:
            output.update(evaluations=evaluations, snapshots=snapshots)
        return output

    def evaluate(point, stage, particle):
        nonlocal best, nfev
        if nfev == maxfev:
            return None
        nfev += 1
        value = objective(case['objective'], point)
        if full_trace:
            evaluations.append({'call': nfev, 'generation': nit + (stage == 'move'),
                                'stage': stage, 'particle': particle,
                                'x': point[:], 'fun': value})
        if best is None or value < best['fun']:
            best = {'x': point[:], 'fun': value}
        return value

    if not free:
        evaluate(case['x0'][:], 'all_fixed', 0)
        return result('converged')

    particles = []
    for row in range(size):
        if 'initial_population' in case:
            point = case['initial_population'][row][:]
        elif row == 0:
            point = case['x0'][:]
        else:
            point = [lower for lower, _ in bounds]
            for index in free:
                point[index] = coordinate(bounds, index, rng.unit())
        value = evaluate(point, 'initialize', row)
        if value is None:
            return result('max_evaluations')
        particles.append({'x': point, 'fun': value, 'velocity': [0.0] * len(free),
                          'personal_x': point[:], 'personal_fun': value})
    if full_trace:
        snapshots.append({'generation': 0, 'particles': copy.deepcopy(particles),
                          'best': copy.deepcopy(best), 'rng_draws': rng.draws})

    def converged():
        values = [particle['fun'] for particle in particles]
        if max(values) - min(values) > fatol:
            return False
        for index in free:
            positions = [particle['x'][index] for particle in particles]
            spread = max(positions) - min(positions)
            if xatol == 0:
                if spread != 0:
                    return False
            elif spread / (bounds[index][1] - bounds[index][0]) > xatol:
                return False
        return all(abs(v) <= xatol for particle in particles for v in particle['velocity'])

    while True:
        if converged():
            return result('converged')
        frozen_best = best['x'][:]
        next_particles = []
        for row, particle in enumerate(particles):
            point = particle['x'][:]
            velocity = []
            for column, index in enumerate(free):
                r1, r2 = rng.unit(), rng.unit()
                lower, upper = bounds[index]
                width = upper - lower
                distance_personal = (particle['personal_x'][index] - particle['x'][index]) / width
                distance_global = (frozen_best[index] - particle['x'][index]) / width
                v = options['inertia'] * particle['velocity'][column] + (
                    options['cognitive'] * r1 * distance_personal
                ) + options['social'] * r2 * distance_global
                v = min(options['velocity_clamp'], max(-options['velocity_clamp'], v))
                if v != 0.0:
                    unit = (particle['x'][index] - lower) / width + v
                    if unit <= 0.0:
                        point[index] = lower
                        if v < 0.0:
                            v = 0.0
                    elif unit >= 1.0:
                        point[index] = upper
                        if v > 0.0:
                            v = 0.0
                    else:
                        point[index] = coordinate(bounds, index, unit)
                velocity.append(v)
            value = evaluate(point, 'move', row)
            if value is None:
                return result('max_evaluations')
            updated = copy.deepcopy(particle)
            updated.update(x=point, fun=value, velocity=velocity)
            if value < particle['personal_fun']:
                updated.update(personal_x=point[:], personal_fun=value)
            next_particles.append(updated)
        particles = next_particles
        nit += 1
        callbacks.append({'x': best['x'][:], 'fun': best['fun'], 'nit': nit,
                          'nfev': nfev, 'njev': 0})
        if full_trace:
            snapshots.append({'generation': nit, 'particles': copy.deepcopy(particles),
                              'best': copy.deepcopy(best), 'rng_draws': rng.draws})
        if case.get('cancel_after_generation') == nit:
            return result('cancelled')
        if nit >= maxiter:
            return result('max_iterations')


def fixtures():
    signed = {'name': 'signed_explicit_seed_zero', 'objective': 'signed_quadratic',
              'x0': [0.0, 0.0, 7.0], 'bounds': [[-4.0, 4.0], [-3.0, 3.0], [7.0, 7.0]],
              'initial_population': [[3.0, 1.0, 7.0], [-2.0, -2.0, 7.0],
                                     [0.0, 2.0, 7.0], [1.0, -1.0, 7.0]],
              'seed': 0, 'xatol': 0.0, 'fatol': 0.0, 'maxiter': 3}
    short = [signed]
    for name, changes in [
        ('zero_coefficients', {'coefficients': {'inertia': 0.0, 'cognitive': 0.0, 'social': 0.0}, 'maxiter': 2}),
        ('partial_initialization_budget', {'maxfev': 2}),
        ('partial_first_generation_budget', {'maxfev': 7}),
        ('completed_generation_budget', {'maxfev': 8}),
        ('callback_cancels_at_maxiter', {'maxiter': 1, 'cancel_after_generation': 1}),
        ('random_seed_zero', {'population_size': 4, 'maxiter': 2}),
        ('random_partial_generation_preserves_improvement', {'population_size': 4, 'maxiter': 2, 'maxfev': 5}),
    ]:
        case = copy.deepcopy(signed)
        case.update(name=name, **changes)
        if name.startswith('random_'):
            del case['initial_population']
        short.append(case)
    short += [
        {'name': 'boundary_clipping_zeroes_outward_velocity', 'objective': 'boundary_quadratic',
         'x0': [0.5], 'bounds': [[0.0, 1.0]], 'initial_population': [[0.0], [0.5], [0.9], [1.0]],
         'seed': 0, 'xatol': 0.0, 'fatol': 0.0, 'maxiter': 3,
         'coefficients': {'inertia': 1.0, 'cognitive': 4.0, 'social': 4.0, 'velocity_clamp': 1.0}},
        {'name': 'strict_ties_keep_earliest', 'objective': 'constant',
         'x0': [0.0], 'bounds': [[-1.0, 1.0]], 'initial_population': [[0.2], [-0.8], [0.8], [0.1]],
         'seed': 0, 'xatol': 0.0, 'fatol': 0.0, 'maxiter': 2},
        {'name': 'velocity_prevents_false_convergence', 'objective': 'constant',
         'x0': [0.0], 'bounds': [[-1.0, 1.0]], 'initial_population': [[0.2], [-0.8], [0.8], [0.1]],
         'seed': 0, 'xatol': 0.15, 'fatol': 0.0, 'maxiter': 3},
        {'name': 'rounded_normalization_does_not_converge', 'objective': 'rounded_normalization',
         'x0': [0.0], 'bounds': [[-1e308, 7e307]],
         'initial_population': [[0.0], [1e-100], [-1e-100], [math.nextafter(0.0, 1.0)]],
         'seed': 0, 'xatol': 0.0, 'fatol': 0.0, 'maxiter': 2,
         'coefficients': {'inertia': 0.0, 'cognitive': 0.0, 'social': 0.0}},
        {'name': 'all_fixed_evaluates_x0_once', 'objective': 'signed_quadratic',
         'x0': [1.25, -0.75], 'bounds': [[1.25, 1.25], [-0.75, -0.75]],
         'initial_population': [[1.25, -0.75]] * 4, 'seed': 0},
    ]
    quality = [
        {'name': 'signed_analytic_quality', 'objective': 'signed_quadratic',
         'x0': [0.0, 0.0], 'bounds': [[-4.0, 4.0], [-3.0, 3.0]],
         'analytic_x': [1.25, -0.75], 'analytic_fun': -3.0},
        {'name': 'multimodal_polynomial_analytic_quality', 'objective': 'multimodal_polynomial',
         'x0': [-2.0], 'bounds': [[-3.0, 3.0]], 'analytic_x': [1.0], 'analytic_fun': 0.0},
        {'name': 'himmelblau_analytic_quality', 'objective': 'himmelblau',
         'x0': [0.0, 0.0], 'bounds': [[-5.0, 5.0], [-5.0, 5.0]],
         'analytic_x': [3.0, 2.0], 'analytic_fun': 0.0, 'multiple_minima': True,
         'minima': [[3.0, 2.0], [-2.805118086952745, 3.131312518250573],
                    [-3.779310253377747, -3.283185991286169], [3.5844283403304917, -1.8481265269644036]]},
        {'name': 'shifted_rastrigin_analytic_quality', 'objective': 'shifted_rastrigin',
         'x0': [2.4, -1.7], 'bounds': [[-5.12, 5.12], [-5.12, 5.12]],
         'analytic_x': [0.0, 0.0], 'analytic_fun': -5.0},
    ]
    for case in quality:
        case.update(seed=42, population_size=64, xatol=1e-6, fatol=1e-8, maxiter=1000)
    return short, quality


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path(__file__).resolve().parents[3]
                        / "crates/itofin-optimize/tests/fixtures/particle_swarm.jsonl")
    args = parser.parse_args()
    short, quality = fixtures()
    cases = []
    for case in short + quality:
        traced = case in short
        expected = run(case, full_trace=traced)
        expected.pop("snapshots", None)
        expected.pop("rng_draws", None)
        if traced:
            expected["evaluations"] = [{"x": e["x"], "fun": e["fun"]}
                                       for e in expected["evaluations"]]
        else:
            expected.pop("callbacks", None)
        cases.append({"input": case, "expected": expected, "trace": traced})
    provenance = {"implementation": "Independent standard-library synchronous global-best equations",
                  "references": ["https://doi.org/10.1109/ICNN.1995.488968",
                                 "https://doi.org/10.1109/ICEC.1998.699146"],
                  "comparison": "Exact short-policy traces; analytic multimodal quality, no optimum guarantee"}
    provenance["format"] = "JSONL: one auditable input, result, objective call or callback per record"
    records = [{"kind": "provenance", "value": provenance}]
    for case in cases:
        name = case["input"]["name"]
        records.append({"kind": "input", "name": name, "value": case["input"], "trace": case["trace"]})
        expected = case["expected"]
        evaluations = expected.pop("evaluations", [])
        callbacks = expected.pop("callbacks", [])
        records.append({"kind": "result", "name": name, "value": expected})
        for kind, values in [("evaluation", evaluations), ("callback", callbacks)]:
            records.extend({"kind": kind, "name": name, "value": value} for value in values)
    args.output.write_text("\n".join(json.dumps(record, allow_nan=False) for record in records) + "\n")
    print(f"Generated {len(short)} policy traces and {len(quality)} analytic-quality cases")


if __name__ == "__main__":
    main()
