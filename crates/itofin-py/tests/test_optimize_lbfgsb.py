"""Bounded L-BFGS-B across the Python optimizer interface."""

import numpy as np
import pytest

from itofin import ItofinError
from itofin.optimize import Status, minimize
from itofin.quotes import SimpleQuote


def quadratic(x: np.ndarray) -> float:
    return float((x[0] - 3.0) ** 2 + (x[1] + 2.0) ** 2)


def test_bound_face_and_open_side_with_numeric_and_analytic_gradients() -> None:
    bounds = [(0.0, 1.0), (None, None)]
    numeric = minimize(quadratic, [0.0, 0.0], method="L-BFGS-B", bounds=bounds)
    analytic = minimize(
        quadratic,
        [0.0, 0.0],
        method="l-bfgs-b",
        bounds=bounds,
        jac=lambda x: [2 * (float(x[0]) - 3), 2 * (float(x[1]) + 2)],
    )
    for result in [numeric, analytic]:
        assert result.success and result.status in (Status.ConvergedGTol, Status.ConvergedFTol)
        np.testing.assert_allclose(result.x, [1.0, -2.0], atol=1e-4)
        assert result.njev > 0
    assert analytic.nfev < numeric.nfev


def test_invalid_bounds_and_gradient_error_identity() -> None:
    with pytest.raises(ValueError, match="bounds length"):
        minimize(quadratic, [0.0, 0.0], method="L-BFGS-B", bounds=[(0.0, 1.0)])
    with pytest.raises(ItofinError, match="lower bound"):
        minimize(quadratic, [0.0, 0.0], method="L-BFGS-B", bounds=[(2.0, 1.0), (None, None)])
    with pytest.raises(ValueError, match="does not support option"):
        minimize(quadratic, [0.0, 0.0], method="L-BFGS-B", options={"xatol": 1e-8})
    sentinel = RuntimeError("gradient failed")

    def fail(_: np.ndarray) -> list[float]:
        raise sentinel

    with pytest.raises(RuntimeError) as caught:
        minimize(quadratic, [0.0, 0.0], method="L-BFGS-B", jac=fail)
    assert caught.value is sentinel


def test_bounded_simple_quote_calibration_inside_objective() -> None:
    quote = SimpleQuote(0.08)
    target = 0.03

    def squared_quote_error(x: np.ndarray) -> float:
        quote.set_value(float(x[0]))
        return (quote.value() - target) ** 2

    result = minimize(
        squared_quote_error,
        [0.08],
        method="L-BFGS-B",
        bounds=[(0.0, 0.1)],
        options={"gtol": 1e-8},
    )
    assert result.success
    quote.set_value(float(result.x[0]))
    assert abs(quote.value() - target) < 1e-4
