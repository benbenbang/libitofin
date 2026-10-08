"""Hybrid annealing preserves signed scalar costs, true counts and Python errors."""

# standard library
import traceback

# pypi/conda library
import numpy as np
import pytest

# itofin library
from itofin import ItofinError
from itofin.optimize import OptimizeResult, Status, minimize
from itofin.quotes import SimpleQuote

METHOD = "Hybrid-Simulated-Annealing"


def signature(result: OptimizeResult) -> tuple:
    """Capture the deterministic solution, counters and termination."""
    return (result.x.tolist(), result.fun, result.nit, result.nfev, result.njev, int(result.status))


@pytest.mark.parametrize("seed", [0, 73, 2**64 - 1])
def test_seeded_signed_cost_and_fixed_coordinate(seed: int) -> None:
    """Negative objective values are minimized directly, with exact fixed coordinates."""
    seen: list[np.ndarray] = []

    def objective(x: np.ndarray) -> float:
        seen.append(x.copy())
        return float((x[0] - 0.37) ** 2 - 3.0)

    kwargs = {"method": METHOD, "bounds": [(-2.0, 2.0), (7.0, 7.0)],
              "options": {"seed": seed, "maxiter": 1000, "maxfev": 10000}}
    first = minimize(objective, [0.0, 7.0], **kwargs)
    assert first.success and first.nfev == len(seen) and first.njev == 0
    assert all(-2.0 <= point[0] <= 2.0 and point[1] == 7.0 for point in seen)
    second = minimize(objective, [0.0, 7.0], **kwargs)
    assert signature(first) == signature(second)
    assert first.fun == pytest.approx(-3.0, abs=1e-8)
    assert first.x[0] == pytest.approx(0.37, abs=1e-4)
    assert first.x[1] == 7.0


@pytest.mark.parametrize("budget", [1, 2, 5])
def test_budget_returns_only_best_evaluated_point(budget: int) -> None:
    """The initial point and local-search probes consume the same finite budget."""
    seen: list[np.ndarray] = []

    def objective(x: np.ndarray) -> float:
        seen.append(x.copy())
        return float((x[0] - 0.1) ** 2 - 5.0)

    result = minimize(objective, [0.8], method=METHOD, bounds=[(-1.0, 1.0)],
                      options={"maxfev": budget, "local_search_interval": 1})
    assert result.status == Status.MaxEvaluations and not result.success
    assert result.nfev == len(seen) == budget and result.njev == 0
    assert result.fun == min(float((point[0] - 0.1) ** 2 - 5.0) for point in seen)
    assert any(np.array_equal(result.x, point) for point in seen)
    np.testing.assert_array_equal(seen[0], [0.8])


def test_singleton_box_evaluates_once_and_result_arrays_are_owned() -> None:
    """A valid all-fixed problem has one evaluation and independent output arrays."""
    result = minimize(lambda x: float(x[0] ** 2), [2.0], method=METHOD, bounds=[(2.0, 2.0)])
    assert result.success and result.nit == 0 and result.nfev == 1 and result.njev == 0
    assert result.fun == 4.0
    changed = result.x
    changed[:] = 999.0
    np.testing.assert_array_equal(result.x, [2.0])


def test_live_quote_and_nested_optimizer_reentry() -> None:
    """User code may reprice and recursively invoke an independent optimizer."""
    quote = SimpleQuote(0.08)

    def objective(x: np.ndarray) -> float:
        nested = minimize(lambda y: float(y[0] ** 2), [2.0], method=METHOD, bounds=[(2.0, 2.0)])
        assert nested.fun == 4.0
        quote.set_value(float(x[0]))
        return (quote.value() - 0.03) ** 2

    result = minimize(objective, [0.08], method=METHOD, bounds=[(0.0, 0.1)], options={"maxfev": 5000})
    quote.set_value(float(result.x[0]))
    assert quote.value() == pytest.approx(0.03, abs=1e-5)


@pytest.mark.parametrize("where", ["objective", "callback"])
def test_python_error_identity_and_traceback_are_retained(where: str) -> None:
    """No wrapper converts callback errors into penalties, NaNs or a new exception."""
    error = LookupError("owned Python sentinel")

    def fail(_: np.ndarray) -> float:
        raise error

    def objective(x: np.ndarray) -> float:
        return float(x[0] ** 2)

    with pytest.raises(LookupError) as captured:
        minimize(fail if where == "objective" else objective, [0.8], method=METHOD,
                 bounds=[(-1.0, 1.0)], callback=fail if where == "callback" else None)
    assert captured.value is error
    assert "fail" in [frame.name for frame in traceback.extract_tb(captured.value.__traceback__)]
    recovered = minimize(objective, [0.8], method=METHOD, bounds=[(-1.0, 1.0)], options={"maxfev": 1})
    assert recovered.nfev == 1


def test_stop_iteration_cancels_after_completed_iteration() -> None:
    """StopIteration is an explicit cancellation with actual work preserved."""
    seen: list[np.ndarray] = []

    def objective(x: np.ndarray) -> float:
        seen.append(x.copy())
        return float(x[0] ** 2)

    def stop(_: np.ndarray) -> None:
        raise StopIteration

    result = minimize(objective, [0.8], method=METHOD, bounds=[(-1.0, 1.0)], callback=stop)
    assert result.status == Status.Cancelled and not result.success and result.nit == 1
    assert result.nfev == len(seen) and result.njev == 0


@pytest.mark.parametrize("value", [float("nan"), float("inf"), float("-inf")])
def test_nonfinite_objective_has_distinct_status(value: float) -> None:
    """Nonfinite numerical output is a result status, not a typed callback failure."""
    result = minimize(lambda _: value, [0.0], method=METHOD, bounds=[(-1.0, 1.0)])
    assert result.status == Status.Nonfinite and not result.success
    assert result.nfev == 1 and result.nit == 0 and result.njev == 0


@pytest.mark.parametrize("options", [
    {"initial_temperature": 0.0}, {"initial_temperature": float("inf")},
    {"cooling_rate": 0.0}, {"cooling_rate": 1.0}, {"cooling_rate": float("nan")},
    {"step_size": 0.0}, {"step_size": 1.1}, {"step_size": float("inf")},
    {"local_search_interval": 0}, {"local_search_interval": 1_000_001},
    {"local_search_steps": 0}, {"local_search_steps": 257}, {"reanneal_interval": 0},
    {"seed": True}, {"seed": -1}, {"seed": 2**64}, {"seed": 1.5},
    {"local_search_interval": True}, {"local_search_steps": False}, {"reanneal_interval": True},
    {"maxiter": True}, {"maxfev": True}, {"maxiter": 0}, {"maxfev": 0},
    {"maxiter": 1_000_001}, {"maxfev": 10_000_001},
    {"xatol": -1.0}, {"fatol": float("nan")},
    {"population_size": 8}, {"initial_population": [[0.0]]}, {"mutation": 0.8},
    {"inertia": 0.7}, {"gtol": 1e-6},
])
def test_invalid_and_unsupported_controls_never_evaluate(options: dict) -> None:
    """Reject schedules, population controls and noninteger integer fields before callbacks."""
    def forbidden(_: np.ndarray) -> float:
        pytest.fail("invalid hybrid annealing options reached the objective")

    with pytest.raises((ItofinError, TypeError, ValueError, OverflowError)):
        minimize(forbidden, [0.0], method=METHOD, bounds=[(-1.0, 1.0)], options=options)


@pytest.mark.parametrize("extra", [
    {}, {"bounds": [(None, 1.0)]}, {"bounds": [(1.0, -1.0)]}, {"bounds": []},
    {"bounds": [(-float("inf"), float("inf"))]},
    {"bounds": [(-1.0, 1.0)], "jac": lambda x: [0.0]},
    {"bounds": [(-1.0, 1.0)], "constraints": []},
])
def test_invalid_bounds_and_unsupported_derivatives_never_evaluate(extra: dict) -> None:
    """Finite explicit boxes are required; general constraints and gradients are rejected."""
    def forbidden(_: np.ndarray) -> float:
        pytest.fail("invalid hybrid annealing problem reached the objective")

    with pytest.raises((ItofinError, TypeError, ValueError)):
        minimize(forbidden, [0.0], method=METHOD, **extra)
