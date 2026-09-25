"""Nelder-Mead through ``itofin.optimize.minimize`` (issue #1081).

The objective runs outside any bootstrap callback, so it may mutate a
``SimpleQuote`` and reprice (D-OPT 7). Errors raised by ``fun`` or ``callback``
come back as the same exception object; ``StopIteration`` from ``callback``
cancels the run instead.
"""

# pypi/conda library
import numpy as np
import pytest

# itofin library
from itofin import ItofinError, Settings
from itofin.indexes import Euribor
from itofin.instruments import MakeVanillaSwap
from itofin.optimize import OptimizeResult, Status, minimize
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward
from itofin.time import Date, DayCounter, Period


def rosenbrock(x: np.ndarray) -> float:
    """The banana function, minimized at (1, 1)."""
    return float(100.0 * (x[1] - x[0] ** 2) ** 2 + (1.0 - x[0]) ** 2)


def test_rosenbrock_converges_to_the_valley_floor() -> None:
    """A tight run reaches (1, 1) and reports a convergence."""
    result = minimize(rosenbrock, [-1.2, 1.0], options={"xatol": 1e-8, "fatol": 1e-8})
    assert isinstance(result, OptimizeResult)
    assert isinstance(result.x, np.ndarray) and result.x.dtype == np.float64
    np.testing.assert_allclose(result.x, [1.0, 1.0], atol=1e-6)
    assert result.success and result.status in (Status.ConvergedXTol, Status.ConvergedFTol)
    assert result.fun < 1e-10 and result.nit > 0 and result.nfev > result.nit and result.njev == 0
    assert result.message.startswith("converged")


def test_budgets_status_values_and_invalid_input() -> None:
    """Budgets end the run unsuccessfully; bad input raises ItofinError."""
    result = minimize(rosenbrock, [-1.2, 1.0], method="nelder-mead", options={"maxiter": 5})
    assert result.status == Status.MaxIterations and result.nit == 5 and not result.success
    expected = ["ConvergedXTol", "ConvergedFTol", "ConvergedGTol", "MaxIterations", "MaxEvaluations"]
    for value, name in enumerate([*expected, "Cancelled", "Nonfinite"]):
        assert int(getattr(Status, name)) == value
    with pytest.raises(ItofinError, match="x0 must not be empty"):
        minimize(rosenbrock, [])
    with pytest.raises(ItofinError, match="does not support option tol"):
        minimize(rosenbrock, [1.0, 1.0], options={"tol": 1e-3})
    with pytest.raises(ItofinError, match="unknown method BFGS"):
        minimize(rosenbrock, [1.0, 1.0], method="BFGS")


def test_objective_exception_is_reraised_as_the_same_object() -> None:
    """The exception fun raised reaches the caller unchanged."""
    raised = []

    def divide(x: np.ndarray) -> float:
        try:
            return 1.0 / (float(x[0]) - float(x[0]))
        except ZeroDivisionError as error:
            raised.append(error)
            raise

    with pytest.raises(ZeroDivisionError) as caught:
        minimize(divide, [1.0])
    assert caught.value is raised[0]


def test_callback_stop_iteration_cancels_and_other_errors_propagate() -> None:
    """StopIteration from callback cancels; any other exception is re-raised."""
    seen = []

    def stop_after_three(xk: np.ndarray) -> None:
        seen.append(xk.copy())
        if len(seen) == 3:
            raise StopIteration

    result = minimize(rosenbrock, [-1.2, 1.0], callback=stop_after_three)
    assert result.status == Status.Cancelled and result.nit == 3 and not result.success
    assert len(result.x) == 2

    error = ValueError("callback failed")

    def fail(xk: np.ndarray) -> None:
        raise error

    with pytest.raises(ValueError) as caught:
        minimize(rosenbrock, [-1.2, 1.0], callback=fail)
    assert caught.value is error


def test_objective_mutates_a_quote_and_reprices_a_swap() -> None:
    """The flat rate that zeroes a 3% swap's NPV is found by repricing inside fun."""
    settings = Settings()
    today = Date(15, 1, 2026)
    settings.set_evaluation_date(today)
    rate = SimpleQuote(0.05)
    curve = FlatForward.from_quote(today, rate, DayCounter.actual365_fixed())
    index = Euribor.six_months(curve, settings)
    swap = MakeVanillaSwap(Period(5, "Years"), index, settings, fixed_rate=0.03).build()

    def squared_npv(x: np.ndarray) -> float:
        rate.set_value(float(x[0]))
        return (swap.npv() * 1e4) ** 2

    result = minimize(squared_npv, [0.05], options={"xatol": 1e-10, "fatol": 1e-12})
    assert result.success
    rate.set_value(float(result.x[0]))
    assert abs(swap.npv()) < 1e-6
    assert abs(result.x[0] - 0.03) < 5e-3
