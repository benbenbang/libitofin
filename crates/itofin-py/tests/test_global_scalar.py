"""Seeded differential evolution through the scalar Python facade."""

import math

import numpy as np
import pytest

from itofin.optimize import OptimizeResult, Status, minimize
from itofin.quotes import SimpleQuote


METHOD = "Differential-Evolution"


def signature(result: OptimizeResult) -> tuple:
    """Capture deterministic outcomes, counts and termination."""
    return (result.x.tolist(), result.fun, result.nit, result.nfev, result.njev, int(result.status))


def test_seed_zero_and_uint64_max_are_reproducible() -> None:
    """Zero is a deterministic seed rather than a request for entropy."""

    def objective(x: np.ndarray) -> float:
        return float((x[0] - 0.37) ** 2 + (x[1] + 0.42) ** 2)

    for seed in [0, 73, 2**64 - 1]:
        kwargs = {
            "method": "differential-evolution",
            "bounds": [(-2.0, 2.0), (-2.0, 2.0)],
            "options": {"seed": seed, "population_size": 12, "maxiter": 40},
        }
        first = minimize(objective, [0.0, 0.0], **kwargs)
        second = minimize(objective, [0.0, 0.0], **kwargs)
        assert signature(first) == signature(second)
        assert first.njev == 0
        assert first.fun < 1e-5


def test_explicit_population_is_evaluated_unchanged_without_x0_insertion() -> None:
    """Rows, including their order, survive Python conversion unchanged."""
    population = [[-1.0, 3.0], [-0.5, 3.0], [0.25, 3.0], [1.0, 3.0]]
    frozen = [row.copy() for row in population]
    seen = []

    def objective(x: np.ndarray) -> float:
        seen.append(x.copy())
        return float((x[0] - 0.125) ** 2)

    result = minimize(
        objective,
        [0.875, 3.0],
        method=METHOD,
        bounds=[(-1.0, 1.0), (3.0, 3.0)],
        options={"initial_population": population, "maxfev": 4},
    )
    assert population == frozen
    np.testing.assert_array_equal(np.array(seen), np.array(population))
    assert result.nfev == 4 and result.nit == 0
    assert result.status == Status.MaxEvaluations and not result.success
    assert result.fun == (0.25 - 0.125) ** 2
    np.testing.assert_array_equal(result.x, [0.25, 3.0])


def test_all_fixed_bounds_have_one_evaluation_and_no_gradient() -> None:
    """A singleton feasible domain bypasses population generation."""
    seen = []

    def objective(x: np.ndarray) -> float:
        seen.append(x.copy())
        return float(x[0] ** 2 + x[1] ** 2)

    result = minimize(objective, [2.0, -3.0], method=METHOD, bounds=[(2.0, 2.0), (-3.0, -3.0)])
    assert result.success and result.nfev == 1 and result.nit == 0 and result.njev == 0
    assert result.fun == 13.0
    np.testing.assert_array_equal(seen, [[2.0, -3.0]])
    np.testing.assert_array_equal(result.x, [2.0, -3.0])


def test_zero_recombination_forces_a_free_coordinate_and_preserves_fixed_coordinate() -> None:
    """CR zero still mutates one free coordinate, with no fixed-coordinate drift."""
    population = [[-1.0, 7.0], [-0.5, 7.0], [0.25, 7.0], [1.0, 7.0]]
    seen = []

    def objective(x: np.ndarray) -> float:
        seen.append(x.copy())
        return float((x[0] - 0.1) ** 2)

    result = minimize(
        objective,
        [0.0, 7.0],
        method=METHOD,
        bounds=[(-1.0, 1.0), (7.0, 7.0)],
        options={"initial_population": population, "recombination": 0.0, "maxiter": 1},
    )
    assert result.status == Status.MaxIterations and result.nit == 1 and result.nfev == 8
    assert len(seen) == 8
    assert all(point[1] == 7.0 and -1.0 <= point[0] <= 1.0 for point in seen)
    assert any(seen[index + 4][0] != population[index][0] for index in range(4))


def test_negative_multimodal_objective_reaches_the_global_basin() -> None:
    """Population search finds a nonlocal basin without relative-fun tolerance."""

    def objective(x: np.ndarray) -> float:
        value = float(x[0])
        return min((value - 2.0) ** 2 - 10.0, (value + 2.0) ** 2 - 1.0)

    result = minimize(
        objective,
        [-2.0],
        method=METHOD,
        bounds=[(-4.0, 4.0)],
        options={"seed": 19, "population_size": 24},
    )
    assert result.success
    assert result.status in (Status.ConvergedXTol, Status.ConvergedFTol)
    assert result.fun < -9.999999
    assert abs(result.x[0] - 2.0) < 1e-3
    assert math.isfinite(result.fun)


def test_live_simple_quote_can_be_mutated_in_the_objective() -> None:
    """Global objective execution retains the existing quote mutation boundary."""
    quote = SimpleQuote(0.08)

    def objective(x: np.ndarray) -> float:
        quote.set_value(float(x[0]))
        return (quote.value() - 0.03) ** 2

    result = minimize(objective, [0.08], method=METHOD, bounds=[(0.0, 0.1)])
    assert result.success
    quote.set_value(float(result.x[0]))
    assert abs(quote.value() - 0.03) < 1e-4


def test_result_array_is_owned_and_independent_on_each_access() -> None:
    """Mutating a returned array never modifies the result's retained point."""
    result = minimize(lambda x: float(x[0] ** 2), [0.0], method=METHOD, bounds=[(-1.0, 1.0)])
    original = result.x.copy()
    modified = result.x
    assert modified.dtype == np.float64
    modified[:] = 999.0
    np.testing.assert_array_equal(result.x, original)


def test_initialization_budget_returns_the_best_evaluated_row_only() -> None:
    """Partial initialization never reports an unevaluated population member."""
    population = [[-1.0], [-0.5], [0.25], [0.1]]
    seen = []

    def objective(x: np.ndarray) -> float:
        seen.append(x.copy())
        return float((x[0] - 0.1) ** 2)

    result = minimize(
        objective,
        [0.0],
        method=METHOD,
        bounds=[(-1.0, 1.0)],
        options={"initial_population": population, "maxfev": 3},
    )
    assert result.status == Status.MaxEvaluations and not result.success
    assert result.nfev == 3 and result.nit == 0
    np.testing.assert_array_equal(seen, population[:3])
    np.testing.assert_array_equal(result.x, population[2])
    assert result.fun == (0.25 - 0.1) ** 2


def test_flat_objective_alone_is_not_spread_convergence() -> None:
    """Function spread and normalized coordinate spread must both be small."""
    result = minimize(
        lambda _: -5.0,
        [0.0],
        method=METHOD,
        bounds=[(-1.0, 1.0)],
        options={"initial_population": [[-1.0], [-0.5], [0.25], [1.0]], "maxfev": 4},
    )
    assert result.status == Status.MaxEvaluations and not result.success
    assert result.nfev == 4 and result.nit == 0 and result.fun == -5.0
