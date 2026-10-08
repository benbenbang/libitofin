"""Hybrid annealing calibrates model residuals with projected bounds and rollback."""

# standard library
import gc
import math

# pypi/conda library
import pytest
from test_global_calibration import _market

# itofin library
from itofin import ItofinError
from itofin.optimization import BoundaryConstraint, HybridSimulatedAnnealing
from itofin.optimize import Status


def method(**options):
    """Construct one-dimensional projected sigma calibration controls."""
    return HybridSimulatedAnnealing([(0.001, 0.03)], seed=42, xatol=1e-7, fatol=1e-12, **options)


def test_repeated_model_calibration_and_independent_diagnostics() -> None:
    """Seeded repeated sessions retain reversion and return owned result copies."""
    solver = method()
    assert solver.last_result() is None
    snapshots = []
    for _ in range(2):
        model, helpers, criteria = _market()
        gc.collect()
        model.calibrate(helpers, solver, criteria, True)
        result = solver.last_result()
        assert result is not None
        assert model.a() == 0.05
        assert model.sigma() == pytest.approx(0.00585858, abs=1e-5)
        assert result.x.tolist() == [model.sigma()]
        assert result.fun >= 0.0 and result.nfev > 1 and result.njev == 0
        root_sum_squares = math.sqrt(sum(helper.calibration_error() ** 2 for helper in helpers))
        assert result.fun == pytest.approx(root_sum_squares, rel=2e-12, abs=1e-14)
        snapshots.append((result.x.tolist(), result.fun, result.nit, result.nfev, int(result.status)))
        result.x[0] = 999
        copied = solver.last_result()
        assert copied is not None and copied.x[0] == model.sigma()
        with pytest.raises(ItofinError):
            model.calibrate([], solver, criteria, True)
        retained = solver.last_result()
        assert retained is not None and retained.nfev == result.nfev
    assert snapshots[0] == snapshots[1]


def test_exhaustion_is_not_convergence_and_retains_original_point() -> None:
    """A single evaluation reports the actual RSS without pretending to converge."""
    model, helpers, criteria = _market()
    before = (model.a(), model.sigma())
    solver = method(maxfev=1)
    model.calibrate(helpers, solver, criteria, True)
    result = solver.last_result()
    assert result is not None and result.status == Status.MaxEvaluations
    assert not result.success and result.nfev == 1 and result.nit == 0 and result.njev == 0
    assert result.fun >= 0.0 and result.x.tolist() == [before[1]]
    assert (model.a(), model.sigma()) == before


def test_dimension_failure_clears_old_result_and_restores_model() -> None:
    """A stale success or exhaustion must not survive a later dimension failure."""
    model, helpers, criteria = _market()
    solver = method(maxfev=1)
    model.calibrate(helpers, solver, criteria, True)
    before = (model.a(), model.sigma())
    with pytest.raises(ItofinError, match="free parameter count"):
        model.calibrate(helpers, solver, criteria, False)
    assert solver.last_result() is None
    assert (model.a(), model.sigma()) == before


def test_intersected_box_excludes_start_and_rolls_back() -> None:
    """The model start must fit the effective box before any pricing."""
    model, helpers, criteria = _market()
    before = (model.a(), model.sigma())
    solver = method()
    with pytest.raises(ItofinError):
        model.calibrate(helpers, solver, criteria, True,
                        constraint=BoundaryConstraint(0.001, 0.005))
    assert solver.last_result() is None
    assert (model.a(), model.sigma()) == before


@pytest.mark.parametrize("options", [
    {"seed": True}, {"seed": -1}, {"seed": 2**64},
    {"maxiter": True}, {"maxfev": True}, {"maxfev": 10_000_001},
    {"initial_temperature": 0.0}, {"cooling_rate": 1.0}, {"step_size": 0.0},
    {"local_search_interval": True}, {"local_search_interval": 0},
    {"local_search_steps": False}, {"local_search_steps": 257},
    {"reanneal_interval": True}, {"reanneal_interval": 0},
    {"xatol": float("nan")}, {"fatol": -1.0},
    {"population_size": 8}, {"initial_population": [[0.0]]},
])
def test_invalid_constructor_controls(options: dict) -> None:
    """Schedule validation and integer extraction happen at method construction."""
    with pytest.raises((ItofinError, TypeError, ValueError, OverflowError)):
        HybridSimulatedAnnealing([(0.001, 0.03)], **options)
