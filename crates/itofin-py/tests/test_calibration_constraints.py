"""Cross-binding calibration constraints and per-fit options."""

import gc
import json
import math
import os
from typing import cast

import pytest

from itofin import ItofinError, Settings
from itofin.models import CalibrationErrorType, HestonModel, HestonModelHelper, HullWhite
from itofin.optimization import (
    BoundaryConstraint,
    CompositeConstraint,
    EndCriteria,
    LevenbergMarquardt,
    NoConstraint,
    PositiveConstraint,
)
from itofin.processes import HestonProcess
from itofin.termstructures import FlatForward
from itofin.time import Calendar, Date, DayCounter, Period


QUOTES = (
    (6, 80, 0.23648109919405938),
    (6, 100, 0.2002947592621531),
    (6, 120, 0.18031832589884542),
    (12, 80, 0.23146092911030172),
    (12, 100, 0.2025441016279331),
    (12, 120, 0.183784462198752),
    (24, 80, 0.22726138410502447),
    (24, 100, 0.20640988517573639),
    (24, 120, 0.19266034047177458),
)


def _market():
    reference = Date(15, 1, 2026)
    day_counter = DayCounter.actual365_fixed()
    settings = Settings()
    settings.set_evaluation_date(reference)
    helpers = [
        HestonModelHelper(
            Period(months, "Months"),
            Calendar.null_calendar(),
            100.0,
            strike,
            volatility,
            0.03,
            0.01,
            CalibrationErrorType.PriceError,
            reference,
            day_counter,
            settings,
        )
        for months, strike, volatility in QUOTES
    ]

    def model():
        return HestonModel(HestonProcess(0.03, 0.01, 100.0, 0.035, 1.0, 0.045, 0.25, -0.4, reference, day_counter))

    return helpers, model, EndCriteria(1000, 100, 1e-8, 1e-8, 1e-8)


def _parameters(model):
    return [model.theta(), model.kappa(), model.sigma(), model.rho(), model.v0()]


@pytest.mark.parametrize(
    "option,expected",
    [
        (
            "boundary",
            [
                0.046303979955933196,
                1.0999903505348982,
                0.23554117657339227,
                -0.4022760903225963,
                0.0362696716686095,
            ],
        ),
        (
            "fixed",
            [
                0.05223845975417122,
                1.0685323850722415,
                0.3516343242325526,
                -0.4,
                0.04016953450282646,
            ],
        ),
        (
            "weights",
            [
                0.04987723883161238,
                1.241266822131491,
                0.30695619993942197,
                -0.49495452072840185,
                0.03999130931061108,
            ],
        ),
    ],
)
def test_heston_options_match_go_and_core(option, expected):
    """Match the same constrained, fixed and weighted Go/core market."""
    helpers, model_factory, criteria = _market()
    model = model_factory()
    if option == "boundary":
        model.calibrate(
            helpers,
            LevenbergMarquardt(),
            criteria,
            96,
            constraint=CompositeConstraint(NoConstraint(), BoundaryConstraint(-0.75, 1.1)),
        )
    elif option == "fixed":
        model.calibrate(
            helpers,
            LevenbergMarquardt(),
            criteria,
            96,
            fix_parameters=[False, False, False, True, False],
        )
    else:
        model.calibrate(
            helpers,
            LevenbergMarquardt(),
            criteria,
            96,
            weights=[5.0] * 3 + [1.0] * 6,
        )
    oracle_json = os.getenv("ITOFIN_HCAL_CORE_ORACLE")
    if oracle_json:
        core = json.loads(oracle_json)
        assert len(core) == 3 and all(len(row) == 5 for row in core)
        assert _parameters(model) == pytest.approx(
            core[["boundary", "fixed", "weights"].index(option)], rel=0, abs=1e-12
        )
    elif os.getenv("ITOFIN_RELEASE_PARITY") == "1":
        assert _parameters(model) == pytest.approx(expected, rel=0, abs=1e-12)
    elif option == "boundary":
        assert 1.099 < model.kappa() <= 1.1 + 1e-12
    elif option == "fixed":
        assert model.rho() == -0.4
    else:
        unweighted = model_factory()
        unweighted.calibrate(helpers, LevenbergMarquardt(), criteria, 96)
        assert abs(model.kappa() - unweighted.kappa()) > 1e-5
        assert all(math.isfinite(value) for value in _parameters(model))


def test_composite_copies_children_and_reuses_across_fits():
    """Composite ownership survives child collection and repeated fits."""
    helpers, model_factory, criteria = _market()
    left = NoConstraint()
    right = BoundaryConstraint(-0.75, 1.1)
    constraint = CompositeConstraint(left, right)
    del left, right
    gc.collect()
    results = []
    for _ in range(2):
        model = model_factory()
        model.calibrate(helpers, LevenbergMarquardt(), criteria, 96, constraint=constraint)
        results.append(_parameters(model))
    assert results[0] == pytest.approx(results[1], rel=0, abs=1e-12)


def test_constraint_construction_and_core_input_errors():
    """Reject bad bounds and preserve core size/projection diagnostics."""
    for low, high in ((2, 1), (math.nan, 1), (-1, math.inf)):
        with pytest.raises(ValueError, match="finite and ordered"):
            BoundaryConstraint(low, high)
    with pytest.raises(TypeError, match="constraint must be"):
        CompositeConstraint(NoConstraint(), cast(NoConstraint, object()))

    helpers, model_factory, criteria = _market()
    for fit, message in (
        (
            lambda: model_factory().calibrate(helpers, LevenbergMarquardt(), criteria, 96, weights=[1.0, 1.0]),
            "mismatch between number of instruments",
        ),
        (
            lambda: model_factory().calibrate(
                helpers, LevenbergMarquardt(), criteria, 96, fix_parameters=[True, False]
            ),
            "mismatch between number of parameters",
        ),
        (
            lambda: model_factory().calibrate(helpers, LevenbergMarquardt(), criteria, 96, fix_parameters=[True] * 5),
            "numberOfFreeParameters==0",
        ),
    ):
        with pytest.raises(ItofinError, match=message):
            fit()
    with pytest.raises(TypeError, match="constraint must be"):
        model_factory().calibrate(
            helpers,
            LevenbergMarquardt(),
            criteria,
            96,
            constraint=cast(NoConstraint, object()),
        )


def test_all_calibration_paths_accept_keyword_options():
    """Route options through every facade and reject ambiguous Hull-White masks."""
    helpers, model_factory, criteria = _market()
    model = model_factory()
    method = LevenbergMarquardt()
    options = {"constraint": NoConstraint(), "weights": [], "fix_parameters": []}
    reference = Date(15, 1, 2026)
    curve = FlatForward(reference, 0.03, DayCounter.actual365_fixed())
    hull_white = HullWhite(curve, 0.05, 0.01)
    for fit in (
        lambda: model.calibrate(helpers[:0], method, criteria, 96, **options),
        lambda: model.calibrate_cos(helpers[:0], method, criteria, **options),
        lambda: model.calibrate_exponential_fitting(helpers[:0], method, criteria, **options),
        lambda: hull_white.calibrate(
            [],
            method,
            criteria,
            False,
            constraint=PositiveConstraint(),
            weights=[],
            fix_parameters=[],
        ),
        lambda: hull_white.calibrate_caps([], method, criteria, False, 30, **options),
    ):
        with pytest.raises(ItofinError, match="no instruments provided"):
            fit()
    for fit in (
        lambda: hull_white.calibrate([], method, criteria, True, fix_parameters=[False, True]),
        lambda: hull_white.calibrate_caps([], method, criteria, True, 30, fix_parameters=[False, True]),
    ):
        with pytest.raises(ValueError, match="fix_reversion and fix_parameters"):
            fit()
