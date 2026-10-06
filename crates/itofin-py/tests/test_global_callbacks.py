"""Callback cancellation, exceptions and reentry for global scalar optimization."""

import numpy as np
import pytest

from itofin.optimize import Status, minimize


METHOD = "Differential-Evolution"
BOUNDS = [(-1.0, 1.0)]
POPULATION = [[-1.0], [-0.5], [0.25], [1.0]]


def quadratic(x: np.ndarray) -> float:
    """A nonconstant bounded objective whose optimum is not an initial row."""
    return float((x[0] - 0.1) ** 2)


def test_callback_stop_iteration_cancels_after_one_generation() -> None:
    """Cancellation retains the best point and counts completed work."""
    seen = []

    def stop(x: np.ndarray) -> None:
        seen.append(x.copy())
        raise StopIteration

    result = minimize(
        quadratic,
        [0.0],
        method=METHOD,
        bounds=BOUNDS,
        options={"initial_population": POPULATION},
        callback=stop,
    )
    assert result.status == Status.Cancelled and not result.success
    assert result.nit == 1 and result.nfev == 8 and result.njev == 0
    assert len(seen) == 1
    np.testing.assert_array_equal(result.x, seen[0])


@pytest.mark.parametrize("callback_failure", [False, True])
def test_objective_and_callback_exception_identity(callback_failure: bool) -> None:
    """The exact Python exception object crosses the Rust boundary unchanged."""
    sentinel = RuntimeError("global callback failure" if callback_failure else "global objective failure")

    def fail(_: np.ndarray) -> float:
        raise sentinel

    with pytest.raises(RuntimeError) as caught:
        minimize(
            quadratic if callback_failure else fail,
            [0.0],
            method=METHOD,
            bounds=BOUNDS,
            options={"initial_population": POPULATION},
            callback=fail if callback_failure else None,
        )
    assert caught.value is sentinel


def test_objective_stop_iteration_remains_an_exception_not_cancellation() -> None:
    """Only callback StopIteration has cancellation semantics."""
    sentinel = StopIteration("objective exhausted")

    def fail(_: np.ndarray) -> float:
        raise sentinel

    with pytest.raises(StopIteration) as caught:
        minimize(fail, [0.0], method=METHOD, bounds=BOUNDS)
    assert caught.value is sentinel


def test_reentry_from_callback_and_mutation_do_not_change_outer_state() -> None:
    """Independent nested runs and writable arrays cannot corrupt the outer run."""
    nested = []

    def callback(x: np.ndarray) -> None:
        inner = minimize(quadratic, [0.2], method="Nelder-Mead")
        nested.append(inner)
        x[:] = 500.0
        raise StopIteration

    result = minimize(
        quadratic,
        [0.0],
        method=METHOD,
        bounds=BOUNDS,
        options={"initial_population": POPULATION},
        callback=callback,
    )
    assert len(nested) == 1 and nested[0].success
    assert result.status == Status.Cancelled
    assert -1.0 <= result.x[0] <= 1.0
    assert result.fun == quadratic(result.x)


def test_objective_array_mutation_is_isolated_from_core_population() -> None:
    """The facade passes owned arrays, not aliases of candidate storage."""
    captured = []

    def objective(x: np.ndarray) -> float:
        value = quadratic(x)
        captured.append(x)
        x[:] = 500.0
        return value

    result = minimize(
        objective,
        [0.0],
        method=METHOD,
        bounds=BOUNDS,
        options={"initial_population": POPULATION, "maxiter": 1},
    )
    assert result.fun == quadratic(result.x)
    assert -1.0 <= result.x[0] <= 1.0
    assert all(point[0] == 500.0 for point in captured)


def test_nan_and_infinity_results_stop_without_panicking() -> None:
    """Nonfinite objective results return the established Nonfinite status."""
    for value in [float("nan"), float("inf"), -float("inf")]:
        result = minimize(lambda _: value, [0.0], method=METHOD, bounds=BOUNDS)
        assert result.status == Status.Nonfinite and not result.success
        assert result.nfev == 1


def test_objective_can_reenter_optimizer_without_global_state() -> None:
    """Nested objective runs retain independent solver and callback state."""
    count = 0

    def objective(x: np.ndarray) -> float:
        nonlocal count
        count += 1
        inner = minimize(lambda y: float((y[0] - 0.2) ** 2), [0.0])
        assert inner.success
        return quadratic(x)

    result = minimize(
        objective,
        [0.0],
        method=METHOD,
        bounds=BOUNDS,
        options={"initial_population": POPULATION, "maxfev": 4},
    )
    assert count == 4 and result.nfev == 4
    assert result.status == Status.MaxEvaluations
    assert result.fun == quadratic(result.x)
