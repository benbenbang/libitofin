"""GJR calibration pairs existing Black-vol helpers and optimizer facades."""

import math
from typing import Any, cast

import pytest

from itofin import ItofinError
from itofin.models import CalibrationErrorType, HestonModelHelper
from itofin.optimization import BoundaryConstraint, EndCriteria, LevenbergMarquardt, Simplex
from itofin.time import Calendar, DayCounter, Period
from test_gjr_model import MARKET, PARAMS, REFERENCE, market


def helper(settings, strike=100.0, volatility=0.25):
    """Create the existing reusable Black-vol calibration helper."""
    return HestonModelHelper(
        Period(30, "Days"),
        Calendar.null_calendar(),
        MARKET[0],
        strike,
        volatility,
        MARKET[1],
        MARKET[2],
        CalibrationErrorType.PriceError,
        REFERENCE,
        DayCounter.actual360(),
        settings,
    )


def test_empty_helpers_and_invalid_options_leave_parameters_unchanged():
    """Input validation and unsupported optimizers never partially fit a model."""
    settings, _, _, _, model, _, option = market()
    before = option.npv()
    method, criteria = Simplex(0.01), EndCriteria(100, 20, 1e-8, 1e-8, 1e-8)
    with pytest.raises(ItofinError):
        model.calibrate([], method, criteria)
    invalid: tuple[dict[str, Any], ...] = (
        {"fix_parameters": [False] * 5},
        {"fix_parameters": [False] * 7},
        {"fix_parameters": [True] * 6},
        {"weights": [1.0, 2.0]},
        {"weights": [-1.0]},
        {"weights": [math.nan]},
        {"weights": [math.inf]},
    )
    for options in invalid:
        with pytest.raises(ItofinError):
            model.calibrate([helper(settings)], method, criteria, **options)
        assert model.params() == list(PARAMS)
        assert option.npv() == before
    with pytest.raises(TypeError):
        model.calibrate([helper(settings)], cast(Any, object()), criteria)
    with pytest.raises(TypeError):
        model.calibrate([helper(settings)], method, criteria, constraint=cast(Any, object()))
    assert model.params() == list(PARAMS)


@pytest.mark.parametrize("method", [Simplex(0.001), LevenbergMarquardt(1e-8, 1e-8, 1e-8, False)])
def test_weighted_fixed_mask_calibration_changes_only_daily_variance(method):
    """Both public optimizer families drive a live retained analytic engine."""
    settings, _, _, _, model, _, option = market()
    helpers = [helper(settings, strike) for strike in (90.0, 100.0, 110.0)]
    before = option.npv()
    model.calibrate(
        helpers,
        method,
        EndCriteria(250, 30, 1e-8, 1e-8, 1e-8),
        constraint=BoundaryConstraint(-2.0, 3.0),
        weights=[1.0, 2.0, 1.0],
        fix_parameters=[True] * 5 + [False],
    )
    fitted = model.params()
    assert fitted[:5] == list(PARAMS[:5])
    assert fitted[-1] > 0.0
    assert fitted[-1] != PARAMS[-1]
    assert option.npv() != before
    assert all(math.isfinite(h.calibration_error()) for h in helpers)
    *_, rebuilt = market(params=fitted)
    assert option.npv() == pytest.approx(rebuilt.npv(), rel=0, abs=1e-12)


def test_failed_pricing_during_fit_rolls_back_all_parameters():
    """A helper valuation failure cannot strand a tentative optimizer point."""
    settings, _, _, _, model, _, option = market()
    before = option.npv()
    with pytest.raises(ItofinError):
        model.calibrate(
            [helper(settings, strike=-1.0)],
            Simplex(0.001),
            EndCriteria(100, 20, 1e-8, 1e-8, 1e-8),
            fix_parameters=[True] * 5 + [False],
        )
    assert model.params() == list(PARAMS)
    assert option.npv() == before
