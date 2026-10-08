"""The hybrid-annealing adapter participates in every model calibration dispatch."""

# standard library
import math

# pypi/conda library
import pytest
from test_bates import helper as bates_helper
from test_bates import market as bates_market
from test_gjr_model import PARAMS as GJR_PARAMS
from test_gjr_model import market as gjr_market
from test_gjr_model_calibration import helper as gjr_helper
from test_heston_calibration import _build_helpers, _fixture_settings, _seed_model

# itofin library
from itofin import ItofinError
from itofin.optimization import EndCriteria, HybridSimulatedAnnealing
from itofin.optimize import Status


def test_heston_fixed_mask_and_exact_budget_diagnostics() -> None:
    """Projected theta/kappa/sigma/rho/v0 order is retained by the global method."""
    model = _seed_model()(0.3)
    helpers = _build_helpers(_fixture_settings())[:1]
    before = (model.theta(), model.kappa(), model.sigma(), model.rho(), model.v0())
    method = HybridSimulatedAnnealing([(0.001, 0.5)], maxfev=1, seed=2**64 - 1)
    model.calibrate(helpers, method, EndCriteria(100, 20, 1e-8, 1e-8, 1e-8), 32,
                    fix_parameters=[True, True, False, True, True])
    result = method.last_result()
    assert result is not None and result.status == Status.MaxEvaluations
    assert not result.success and result.nfev == 1 and result.nit == 0 and result.njev == 0
    assert result.x.tolist() == [model.sigma()]
    assert (model.theta(), model.kappa(), model.rho(), model.v0()) == (before[0], before[1], before[3], before[4])


def test_bates_budget_and_nonfinite_pricing_rollback() -> None:
    """Retain fresh nonfinite diagnostics while restoring model and live pricing."""
    settings, _, _, _, model, _, option = bates_market()
    fixed = [True] * 4 + [False] + [True] * 3
    method = HybridSimulatedAnnealing([(0.001, 0.1)], maxfev=1)
    criteria = EndCriteria(100, 20, 1e-8, 1e-8, 1e-8)
    before = model.params()
    model.calibrate([bates_helper(settings)], method, criteria, 32, fix_parameters=fixed)
    result = method.last_result()
    assert result is not None and result.nfev == 1 and result.njev == 0
    assert result.status == Status.MaxEvaluations and not result.success
    assert result.x.tolist() == [model.params()[4]]
    assert model.params() == before
    price = option.npv()
    with pytest.raises(ItofinError):
        model.calibrate([bates_helper(settings, strike=-1.0)], method, criteria, 32, fix_parameters=fixed)
    failed = method.last_result()
    assert failed is not None and failed.status == Status.Nonfinite and not failed.success
    assert failed.nfev == 1 and failed.njev == 0 and not math.isfinite(failed.fun)
    assert model.params() == before and option.npv() == price


def test_gjr_daily_variance_projection_and_nonfinite_pricing_rollback() -> None:
    """The six-parameter GJR model preserves fixed parameters and live pricing."""
    settings, _, _, _, model, _, option = gjr_market()
    fixed = [True] * 5 + [False]
    method = HybridSimulatedAnnealing([(1e-6, 0.001)], maxfev=1)
    criteria = EndCriteria(100, 20, 1e-8, 1e-8, 1e-8)
    before = model.params()
    model.calibrate([gjr_helper(settings)], method, criteria, fix_parameters=fixed)
    result = method.last_result()
    assert result is not None and result.nfev == 1 and result.njev == 0
    assert result.status == Status.MaxEvaluations and not result.success
    assert result.x.tolist() == [model.params()[-1]]
    assert model.params() == before == list(GJR_PARAMS)
    assert math.isfinite(result.fun)
    price = option.npv()
    with pytest.raises(ItofinError):
        model.calibrate([gjr_helper(settings, strike=-1.0)], method, criteria, fix_parameters=fixed)
    failed = method.last_result()
    assert failed is not None and failed.status == Status.Nonfinite and not failed.success
    assert failed.nfev == 1 and failed.njev == 0 and not math.isfinite(failed.fun)
    assert model.params() == before and option.npv() == price
