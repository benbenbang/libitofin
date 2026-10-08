"""Firefly preserves typed Python callback errors, cancellation and numerical status."""

# standard library
import traceback

# pypi/conda library
import numpy as np
import pytest

# itofin library
from itofin.optimize import Status, minimize

METHOD = "Firefly"


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




def test_objective_stop_iteration_is_not_callback_cancellation() -> None:
    """Only StopIteration raised by the callback cancels; objective exceptions propagate."""
    error = StopIteration("objective sentinel")

    def objective(_: np.ndarray) -> float:
        raise error

    with pytest.raises(StopIteration) as captured:
        minimize(objective, [0.8], method=METHOD, bounds=[(-1.0, 1.0)])
    assert captured.value is error
