"""Seeded scalar particle swarm, without a residual or gradient adapter."""

import numpy as np
import pytest

from itofin import ItofinError
from itofin.optimize import OptimizeResult, Status, minimize
from itofin.quotes import SimpleQuote


METHOD = "particle-swarm"
POPULATION = [[-1.0, 7.0], [-0.5, 7.0], [0.25, 7.0], [1.0, 7.0]]


def signature(result: OptimizeResult) -> tuple:
    """Capture the deterministic solution, counters and termination."""
    return (result.x.tolist(), result.fun, result.nit, result.nfev, result.njev, int(result.status))


@pytest.mark.parametrize("seed", [0, 73, 2**64 - 1])
def test_signed_objective_is_reproducible_for_full_uint64_seed_range(seed: int) -> None:
    """Minimize a negative scalar directly, not its squared magnitude."""
    def objective(x: np.ndarray) -> float:
        return float((x[0] - 0.37) ** 2 + (x[1] + 0.42) ** 2 - 3.0)

    kwargs = {"method": METHOD, "bounds": [(-2.0, 2.0)] * 2,
              "options": {"seed": seed, "population_size": 24}}
    first = minimize(objective, [0.0, 0.0], **kwargs)
    second = minimize(objective, [0.0, 0.0], **kwargs)
    assert signature(first) == signature(second)
    assert first.success and first.njev == 0
    assert first.fun == pytest.approx(-3.0, abs=1e-9)
    np.testing.assert_allclose(first.x, [0.37, -0.42], atol=1e-4, rtol=0)
    assert first.nfev == 24 * (first.nit + 1)


@pytest.mark.parametrize("budget", [3, 4, 7])
def test_explicit_population_and_partial_generation_return_evaluated_best(budget: int) -> None:
    """Keep physical input rows and return only evaluated points on exhaustion."""
    frozen = [row.copy() for row in POPULATION]
    seen = []

    def objective(x: np.ndarray) -> float:
        seen.append(x.copy())
        return float((x[0] - 0.1) ** 2 - 5.0)

    result = minimize(objective, [0.875, 7.0], method=METHOD, bounds=[(-1.0, 1.0), (7.0, 7.0)],
                      options={"initial_population": POPULATION, "maxfev": budget})
    assert POPULATION == frozen
    np.testing.assert_array_equal(np.array(seen[:min(4, budget)]), np.array(POPULATION[:min(4, budget)]))
    assert len(seen) == result.nfev == budget and result.nit == 0 and result.njev == 0
    assert result.status == Status.MaxEvaluations and not result.success
    assert result.fun == min(float((point[0] - 0.1) ** 2 - 5.0) for point in seen)
    assert any(np.array_equal(result.x, point) for point in seen)
    assert all(point[1] == 7.0 and -1.0 <= point[0] <= 1.0 for point in seen)


def test_explicit_zero_coefficients_preserve_positions_and_tie_order() -> None:
    """Zero coefficients are values, not omitted default controls."""
    seen = []

    def objective(x: np.ndarray) -> float:
        seen.append(x.copy())
        return -5.0

    result = minimize(objective, [0.0, 7.0], method=METHOD, bounds=[(-1.0, 1.0), (7.0, 7.0)],
                      options={"initial_population": POPULATION, "inertia": 0.0, "cognitive": 0.0,
                               "social": 0.0, "velocity_clamp": 1.0, "maxiter": 2})
    assert result.status == Status.MaxIterations and not result.success
    assert result.nit == 2 and result.nfev == 12 and result.fun == -5.0
    np.testing.assert_array_equal(np.array(seen), np.tile(np.array(POPULATION), (3, 1)))
    np.testing.assert_array_equal(result.x, POPULATION[0])


def test_singleton_box_evaluates_once_and_result_arrays_are_owned() -> None:
    """All-fixed bounds skip generation and expose independent result copies."""
    result = minimize(lambda x: float(x[0] ** 2), [2.0], method=METHOD, bounds=[(2.0, 2.0)])
    assert result.success and result.nit == 0 and result.nfev == 1 and result.njev == 0
    assert result.fun == 4.0
    changed = result.x
    changed[:] = 999.0
    np.testing.assert_array_equal(result.x, [2.0])


def test_live_quote_reentry_from_objective() -> None:
    """The scalar binding holds no pricing session borrow over user code."""
    quote = SimpleQuote(0.08)

    def objective(x: np.ndarray) -> float:
        quote.set_value(float(x[0]))
        return (quote.value() - 0.03) ** 2

    result = minimize(objective, [0.08], method=METHOD, bounds=[(0.0, 0.1)])
    assert result.success
    quote.set_value(float(result.x[0]))
    assert quote.value() == pytest.approx(0.03, abs=1e-5)


@pytest.mark.parametrize("key,value", [
    ("inertia", -0.1), ("inertia", 1.1), ("inertia", np.nan),
    ("cognitive", -0.1), ("cognitive", 4.1), ("cognitive", np.inf),
    ("social", -0.1), ("social", 4.1), ("social", np.nan),
    ("velocity_clamp", 0.0), ("velocity_clamp", -0.1), ("velocity_clamp", 1.1),
    ("velocity_clamp", np.inf), ("mutation", 0.8), ("recombination", 0.9),
])
def test_invalid_coefficients_and_de_only_options_never_call_objective(key: str, value: float) -> None:
    """Reject invalid numeric policies and unsupported options before callbacks."""
    def forbidden(_: np.ndarray) -> float:
        pytest.fail("invalid PSO options reached the objective")

    with pytest.raises((ItofinError, ValueError)):
        minimize(forbidden, [0.0], method=METHOD, bounds=[(-1.0, 1.0)], options={key: value})
