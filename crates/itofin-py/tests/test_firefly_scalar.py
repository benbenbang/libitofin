"""Seeded scalar firefly search, without a residual or gradient adapter."""

# pypi/conda library
import numpy as np
import pytest

# itofin library
from itofin import ItofinError
from itofin.optimize import OptimizeResult, Status, minimize
from itofin.quotes import SimpleQuote

METHOD = "Firefly"
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
    assert first.nfev > 24


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


def test_explicit_zero_noise_preserves_positions_and_tie_order() -> None:
    """Zero random noise is a value, and unchanged proposals consume no evaluations."""
    seen = []

    def objective(x: np.ndarray) -> float:
        seen.append(x.copy())
        return -5.0

    result = minimize(objective, [0.0, 7.0], method=METHOD, bounds=[(-1.0, 1.0), (7.0, 7.0)],
                      options={"initial_population": POPULATION, "alpha": 0.0, "gamma": 0.0, "maxiter": 2})
    assert result.status == Status.MaxIterations and not result.success
    assert result.nit == 2 and result.nfev == 4 and result.fun == -5.0
    np.testing.assert_array_equal(np.array(seen), np.array(POPULATION))
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
    ("alpha", -0.1), ("alpha", 1.1), ("alpha", np.nan),
    ("beta0", 0.0), ("beta0", 1.1), ("beta0", np.inf),
    ("gamma", -0.1), ("gamma", 1_000_001), ("gamma", np.nan),
    ("alpha_decay", 0.0), ("alpha_decay", 1.1), ("alpha_decay", np.inf),
    ("mutation", 0.8), ("recombination", 0.9), ("inertia", 0.7),
    ("initial_temperature", 1.0), ("local_search_interval", 10),
    ("seed", True), ("population_size", False), ("maxiter", True), ("maxfev", True),
    ("seed", -1), ("seed", 2**64), ("seed", 1.5), ("population_size", 3),
    ("maxiter", 0), ("maxfev", 10_000_001), ("fatol", np.nan),
])
def test_invalid_controls_and_other_method_options_never_evaluate(key: str, value: float) -> None:
    """Reject invalid controls and other method policies before callbacks."""
    def forbidden(_: np.ndarray) -> float:
        pytest.fail("invalid Firefly options reached the objective")

    with pytest.raises((ItofinError, TypeError, ValueError, OverflowError)):
        minimize(forbidden, [0.0], method=METHOD, bounds=[(-1.0, 1.0)], options={key: value})


@pytest.mark.parametrize("extra", [
    {}, {"bounds": [(None, 1.0)]}, {"bounds": [(1.0, -1.0)]}, {"bounds": []},
    {"bounds": [(-float("inf"), float("inf"))]},
    {"bounds": [(-1.0, 1.0)], "jac": lambda x: [0.0]},
    {"bounds": [(-1.0, 1.0)], "constraints": []},
])
def test_invalid_boxes_and_unsupported_derivatives_never_evaluate(extra: dict) -> None:
    """Population search requires finite explicit bounds, with no gradients or general constraints."""
    def forbidden(_: np.ndarray) -> float:
        pytest.fail("invalid Firefly problem reached the objective")

    with pytest.raises((ItofinError, TypeError, ValueError)):
        minimize(forbidden, [0.0], method=METHOD, **extra)
