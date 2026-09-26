"""SLSQP constraints, callback ownership, and pricing reentry."""

import numpy as np
import pytest

from itofin import ItofinError
from itofin.optimize import Status, minimize
from test_tree_swaption import _option


def test_hs71_scalar_constraints_and_open_bounds() -> None:
    def objective(x: np.ndarray) -> float:
        return float(x[0] * x[3] * (x[0] + x[1] + x[2]) + x[2])

    constraints = [
        {"type": "ineq", "fun": lambda x: float(np.prod(x) - 25.0)},
        {"type": "eq", "fun": lambda x: float(np.dot(x, x) - 40.0)},
    ]
    result = minimize(
        objective,
        [1.0, 5.0, 5.0, 1.0],
        method="slsqp",
        bounds=[(1.0, 5.0), (1.0, 5.0), (1.0, 5.0), (1.0, None)],
        constraints=constraints,
        options={"ftol": 1e-10},
    )
    assert result.success
    assert result.fun == pytest.approx(17.0140173, abs=1e-6)
    np.testing.assert_allclose(result.x, [1.0, 4.7429994, 3.8211503, 1.3794082], atol=1e-3)
    assert abs(constraints[0]["fun"](result.x)) < 1e-7
    assert abs(constraints[1]["fun"](result.x)) < 1e-7
    assert result.njev > 0


def test_vector_constraint_and_one_row_jacobian() -> None:
    result = minimize(
        lambda x: float((x[0] - 3.0) ** 2 + (x[1] + 1.0) ** 2),
        [0.2, 0.3],
        method="SLSQP",
        jac=lambda x: [2.0 * (x[0] - 3.0), 2.0 * (x[1] + 1.0)],
        bounds=[(0.0, 1.0), (0.0, 1.0)],
        constraints=[
            {
                "type": "ineq",
                "fun": lambda x: [float(x[0] + x[1] - 2.0)],
                "jac": lambda x: [[1.0, 1.0]],
            }
        ],
        options={"ftol": 1e-10},
    )
    assert result.success
    np.testing.assert_allclose(result.x, [1.0, 1.0], atol=1e-8)

    result = minimize(
        lambda x: float(x[0] ** 2 + x[1] ** 2),
        [0.1, 0.1],
        method="SLSQP",
        constraints=[
            {
                "type": "ineq",
                "fun": lambda x: [float(x[0]), float(x[1])],
                "jac": lambda x: [[1.0, 0.0], [0.0, 1.0]],
            }
        ],
    )
    assert result.success
    np.testing.assert_allclose(result.x, [0.0, 0.0], atol=1e-4)


def test_infeasible_and_callback_errors_keep_their_meaning() -> None:
    quadratic = lambda x: float(x[0] ** 2 + x[1] ** 2)
    result = minimize(
        quadratic,
        [0.5, 0.5],
        method="SLSQP",
        constraints=[
            {"type": "ineq", "fun": lambda x: float(x[0] - 1.0)},
            {"type": "ineq", "fun": lambda x: float(-x[0])},
        ],
    )
    assert result.status == Status.Infeasible and not result.success
    assert int(Status.Infeasible) == 8

    sentinel = RuntimeError("constraint failed")

    def fail(_: np.ndarray) -> float:
        raise sentinel

    with pytest.raises(RuntimeError) as caught:
        minimize(quadratic, [0.5, 0.5], method="SLSQP", constraints=[{"type": "eq", "fun": fail}])
    assert caught.value is sentinel

    call_count = 0

    def fail_after_probe(x: np.ndarray) -> float:
        nonlocal call_count
        call_count += 1
        if call_count > 1:
            raise sentinel
        return float(x[0] - 1.0)

    with pytest.raises(RuntimeError) as caught:
        minimize(quadratic, [0.5, 0.5], method="SLSQP", constraints=[{"type": "eq", "fun": fail_after_probe}])
    assert caught.value is sentinel


def test_malformed_constraints_rejected_before_objective() -> None:
    calls = 0

    def objective(x: np.ndarray) -> float:
        nonlocal calls
        calls += 1
        return float(x[0] ** 2)

    with pytest.raises(ValueError, match="does not support constraints"):
        minimize(objective, [1.0], constraints=[])
    with pytest.raises(ValueError, match="does not support option"):
        minimize(objective, [1.0], method="SLSQP", options={"eps": 1e-4})
    with pytest.raises(ValueError, match="bounds length"):
        minimize(objective, [1.0], method="SLSQP", bounds=[])
    with pytest.raises(ValueError, match="constraint type"):
        minimize(objective, [1.0], method="SLSQP", constraints=[{"type": "bad", "fun": objective}])
    with pytest.raises(ValueError, match="constraint fun is required"):
        minimize(objective, [1.0], method="SLSQP", constraints=[{"type": "eq"}])
    with pytest.raises(ValueError, match="constraint jac must be callable"):
        minimize(objective, [1.0], method="SLSQP", constraints=[{"type": "eq", "fun": objective, "jac": 2}])
    with pytest.raises(ItofinError, match="x0 must not be empty"):
        minimize(objective, [], method="SLSQP", constraints=[{"type": "eq", "fun": objective}])
    assert calls == 0


def test_constraint_shape_and_jacobian_errors() -> None:
    calls = 0

    def changing_shape(x: np.ndarray) -> list[float]:
        nonlocal calls
        calls += 1
        return [float(x[0])] if calls == 1 else [float(x[0]), 1.0]

    with pytest.raises(ValueError, match="changed output dimension"):
        minimize(lambda x: float(x[0] ** 2), [1.0], method="SLSQP", constraints=[{"type": "eq", "fun": changing_shape}])
    with pytest.raises(ValueError, match="wrong gradient length"):
        minimize(
            lambda x: float(x[0] ** 2),
            [1.0],
            method="SLSQP",
            constraints=[{"type": "eq", "fun": lambda x: float(x[0]), "jac": lambda x: [1.0, 0.0]}],
        )


def test_constrained_swaption_calibration_reprices_inside_objective() -> None:
    option, _, quote = _option(False, 1.0)
    quote.set_value(0.04875825)
    target = option.npv()

    def price_error(x: np.ndarray) -> float:
        quote.set_value(float(x[0]))
        return ((option.npv() - target) / 20.0) ** 2

    result = minimize(
        price_error,
        [0.05],
        method="SLSQP",
        bounds=[(0.01, None)],
        constraints=[{"type": "ineq", "fun": lambda x: float(0.1 - x[0]), "jac": lambda x: [-1.0]}],
        options={"ftol": 1e-12},
    )
    assert result.success
    quote.set_value(float(result.x[0]))
    assert abs(option.npv() - target) < 1e-4
