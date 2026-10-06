"""Global scalar validation rejects inputs before evaluating user functions."""

import numpy as np
import pytest

from itofin import ItofinError
from itofin.optimize import minimize


METHOD = "Differential-Evolution"


@pytest.mark.parametrize(
    "overrides",
    [
        {"bounds": None},
        {"bounds": [(None, 1.0)]},
        {"bounds": [(-1.0, None)]},
        {"bounds": [(-np.inf, 1.0)]},
        {"bounds": [(0.0, np.inf)]},
        {"bounds": [(np.nan, 1.0)]},
        {"bounds": [(2.0, 1.0)]},
        {"bounds": [(-1e308, 1e308)]},
        {"bounds": []},
        {"x0": [2.0]},
        {"x0": [np.nan]},
        {"x0": []},
        {"options": {"population_size": 0}},
        {"options": {"population_size": 3}},
        {"options": {"population_size": 4097}},
        {"options": {"maxiter": 0}},
        {"options": {"maxfev": 0}},
        {"options": {"maxiter": 1_000_001}},
        {"options": {"maxfev": 10_000_001}},
        {"options": {"xatol": -1.0}},
        {"options": {"fatol": -1.0}},
        {"options": {"xatol": np.nan}},
        {"options": {"fatol": np.inf}},
        {"options": {"mutation": 0.0}},
        {"options": {"mutation": -1.0}},
        {"options": {"mutation": 2.1}},
        {"options": {"mutation": np.nan}},
        {"options": {"recombination": -0.1}},
        {"options": {"recombination": 1.1}},
        {"options": {"recombination": np.inf}},
        {"options": {"initial_population": []}},
        {"options": {"initial_population": [[0.0]] * 3}},
        {"options": {"initial_population": [[0.0, 0.0]] * 4}},
        {"options": {"initial_population": [[0.0], [0.5], [], [-0.5]]}},
        {"options": {"initial_population": [[0.0], [0.5], [2.0], [-0.5]]}},
        {"options": {"initial_population": [[0.0], [0.5], [np.nan], [-0.5]]}},
        {"options": {"initial_population": [[0.0]] * 4, "population_size": 5}},
        {"options": {"strategy": "best1bin"}},
        {"options": {"polish": True}},
        {"options": {"adaptive": True}},
        {"jac": lambda _: [0.0]},
        {"constraints": [{"type": "ineq", "fun": lambda _: 1.0}]},
    ],
)
def test_invalid_inputs_do_not_invoke_the_objective(overrides: dict) -> None:
    """Invalid shapes, bounds, limits and combinations fail before a call."""
    calls = []

    def objective(x: np.ndarray) -> float:
        calls.append(x.copy())
        return 0.0

    kwargs = {"x0": [0.0], "method": METHOD, "bounds": [(-1.0, 1.0)]}
    kwargs.update(overrides)
    with pytest.raises((ValueError, ItofinError)):
        minimize(objective, **kwargs)
    assert not calls


@pytest.mark.parametrize("key", ["seed", "population_size", "maxiter", "maxfev"])
@pytest.mark.parametrize("value", [-1, True, False, 1.5, 2**65])
def test_unsigned_integer_options_reject_lossy_or_boolean_conversion(key: str, value: object) -> None:
    """No wraparound, truncation or bool-as-int seed and budget conversion."""
    calls = []

    def objective(x: np.ndarray) -> float:
        calls.append(x.copy())
        return float(x[0] ** 2)

    with pytest.raises((ValueError, OverflowError, TypeError, ItofinError)):
        minimize(objective, [0.0], method=METHOD, bounds=[(-1.0, 1.0)], options={key: value})
    assert not calls


def test_fixed_coordinate_population_mismatch_is_rejected_without_calls() -> None:
    """Every explicit row must match the fixed-coordinate value exactly."""
    calls = []

    def objective(x: np.ndarray) -> float:
        calls.append(x.copy())
        return 0.0

    with pytest.raises(ItofinError):
        minimize(
            objective,
            [0.0, 3.0],
            method=METHOD,
            bounds=[(-1.0, 1.0), (3.0, 3.0)],
            options={"initial_population": [[0.0, 3.0], [0.5, 3.0], [-0.5, 4.0], [1.0, 3.0]]},
        )
    assert not calls


def test_dimension_and_population_cell_caps_are_enforced() -> None:
    """Population allocations have both dimensional and cell-count limits."""

    def forbidden(_: np.ndarray) -> float:
        pytest.fail("invalid input evaluated the objective")

    with pytest.raises(ItofinError):
        minimize(forbidden, [0.0] * 257, method=METHOD, bounds=[(-1.0, 1.0)] * 257)
    with pytest.raises(ItofinError):
        minimize(
            forbidden,
            [0.0] * 256,
            method=METHOD,
            bounds=[(-1.0, 1.0)] * 256,
            options={"population_size": 4096},
        )
