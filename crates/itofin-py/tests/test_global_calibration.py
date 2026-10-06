import gc

import pytest

from itofin import ItofinError
from itofin.models import HullWhite
from itofin.optimization import BoundaryConstraint, DifferentialEvolution, EndCriteria
from itofin.optimize import Status
from itofin.indexes import Euribor

from test_hullwhite_calibration import _build_helpers, _fixture_curve, _fixture_settings


def _market():
    settings = _fixture_settings()
    curve = _fixture_curve()
    index = Euribor.six_months(curve, settings)
    helpers = _build_helpers(curve, index)
    model = HullWhite(curve, 0.05, 0.01)
    criteria = EndCriteria(500, 50, 1e-8, 1e-8, 1e-8)
    return model, helpers, criteria


def _method(**options):
    return DifferentialEvolution(
        [(0.001, 0.03)], seed=42, population_size=8,
        xatol=1e-7, fatol=1e-12, **options,
    )


def test_global_calibration_fixed_parameter_repeat_and_result_copy():
    method = _method()
    assert method.last_result() is None
    results = []
    for _ in range(2):
        model, helpers, criteria = _market()
        gc.collect()
        model.calibrate(helpers, method, criteria, True)
        result = method.last_result()
        assert result is not None and result.success
        assert model.a() == 0.05
        assert model.sigma() == pytest.approx(0.00585858, abs=1e-5)
        assert result.x.tolist() == [model.sigma()]
        assert result.nfev > 8 and result.njev == 0
        assert result.status in (Status.ConvergedXTol, Status.ConvergedFTol)
        results.append((result.x.tolist(), result.fun, result.nit, result.nfev))
        result.x[0] = 999
        copied = method.last_result()
        assert copied is not None and copied.x[0] == model.sigma()
        with pytest.raises(ItofinError):
            model.calibrate([], method, criteria, True)
        retained = method.last_result()
        assert retained is not None and retained.nfev == result.nfev
    assert results[0] == results[1]


def test_global_calibration_exhaustion_is_not_convergence():
    model, helpers, criteria = _market()
    method = _method(maxfev=1)
    model.calibrate(helpers, method, criteria, True)
    result = method.last_result()
    assert result is not None
    assert result.status == Status.MaxEvaluations
    assert not result.success and result.nfev == 1
    assert model.a() == 0.05


def test_global_calibration_dimension_failure_clears_old_result_and_rolls_back():
    model, helpers, criteria = _market()
    method = _method()
    model.calibrate(helpers, method, criteria, True)
    before = (model.a(), model.sigma())
    with pytest.raises(ItofinError, match="free parameter count"):
        model.calibrate(helpers, method, criteria, False)
    assert method.last_result() is None
    assert (model.a(), model.sigma()) == before


def test_global_calibration_constraint_intersection_and_invalid_population():
    model, helpers, criteria = _market()
    method = DifferentialEvolution(
        [(0.001, 0.03)], initial_population=[[0.003], [0.006], [0.02], [0.025]],
    )
    before = (model.a(), model.sigma())
    with pytest.raises(ItofinError):
        model.calibrate(helpers, method, criteria, True,
                        constraint=BoundaryConstraint(0.001, 0.01))
    assert method.last_result() is None
    assert (model.a(), model.sigma()) == before


@pytest.mark.parametrize("options", [
    {"seed": True}, {"population_size": True}, {"maxiter": True}, {"maxfev": True},
    {"seed": -1}, {"maxfev": 10_000_001}, {"population_size": 3},
    {"mutation": 0}, {"recombination": 1.01}, {"xatol": float("nan")},
])
def test_global_calibration_rejects_invalid_controls(options):
    with pytest.raises((ItofinError, TypeError, ValueError, OverflowError)):
        DifferentialEvolution([(0.001, 0.03)], **options)
