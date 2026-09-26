"""BFGS binding and gradient callback coverage for issue #1084."""

import numpy as np
import pytest

from itofin.optimize import Status, minimize
from test_tree_swaption import _option


def quadratic(x: np.ndarray) -> float:
    return float((x[0] - 2.0) ** 2 + 4.0 * (x[1] + 1.0) ** 2)


def gradient(x: np.ndarray) -> list[float]:
    return [2.0 * (float(x[0]) - 2.0), 8.0 * (float(x[1]) + 1.0)]


def test_analytic_and_numeric_gradient_counts() -> None:
    analytic = minimize(quadratic, [0.0, 0.0], method="BFGS", jac=gradient)
    numeric = minimize(quadratic, [0.0, 0.0], method="bfgs")
    for result in [analytic, numeric]:
        assert result.success and result.status == Status.ConvergedGTol
        np.testing.assert_allclose(result.x, [2.0, -1.0], atol=1e-4)
        assert result.njev > 0
    assert analytic.nfev < numeric.nfev


def test_gradient_exception_and_unsupported_inputs() -> None:
    error = RuntimeError("gradient failed")

    def fail(_: np.ndarray) -> list[float]:
        raise error

    with pytest.raises(RuntimeError) as caught:
        minimize(quadratic, [0.0, 0.0], method="BFGS", jac=fail)
    assert caught.value is error
    with pytest.raises(ValueError, match="bounds"):
        minimize(quadratic, [0.0, 0.0], method="BFGS", bounds=[(0.0, 1.0)])
    with pytest.raises(ValueError, match="does not support option xatol"):
        minimize(quadratic, [0.0, 0.0], method="BFGS", options={"xatol": 1e-5})
    with pytest.raises(ValueError, match="wrong gradient length"):
        minimize(quadratic, [0.0, 0.0], method="BFGS", jac=lambda _: [1.0])


def test_hull_white_swaption_reprices_inside_bfgs_objective() -> None:
    option, _, quote = _option(False, 1.0)
    target = option.npv()

    def price_error(x: np.ndarray) -> float:
        quote.set_value(float(x[0]))
        return ((option.npv() - target) / 200.0) ** 2

    result = minimize(price_error, [0.05], method="BFGS", options={"eps": 1e-8, "gtol": 1e-8})
    assert result.success
    quote.set_value(float(result.x[0]))
    assert abs(option.npv() - target) < 1e-4
