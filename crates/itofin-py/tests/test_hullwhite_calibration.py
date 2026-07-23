import itofin
import pytest

# ORACLE testCachedHullWhite (shortratemodels.cpp:83-153, mirrored by the Rust
# oracle calibrate_cached_hull_white in hullwhite.rs:793-912): calibrate
# Hull-White to five co-terminal swaptions through the analytic Jamshidian engine
# via Levenberg-Marquardt and reproduce the cached a/sigma to 1.3e-5.
#
# The C++/Rust oracle gates the cached values on usingAtParCoupons: the par arm
# (usingAtParCoupons == true) yields a = 0.0464041, sigma = 0.00579912. `true` is
# the Settings default (settings.rs Default), so a plain itofin.Settings() hits
# the par arm without any further wiring. The evaluation date is 15-Feb-2002 on
# Settings, while the flat curve is referenced at the SETTLEMENT date 19-Feb-2002;
# these differ by design (a FlatForward with an explicit reference date is not
# tied to the evaluation date), so both are set independently below.
TODAY = (15, 2, 2002)
SETTLEMENT = (19, 2, 2002)
CURVE_RATE = 0.04875825

# maturity years, length years, Black volatility (shortratemodels.cpp:96-102).
SWAPTIONS = [
    (1, 5, 0.1148),
    (2, 4, 0.1108),
    (3, 3, 0.1070),
    (4, 2, 0.1021),
    (5, 1, 0.1000),
]


def _fixture_settings():
    settings = itofin.Settings()
    settings.set_evaluation_date(itofin.Date(*TODAY))
    return settings


def _fixture_curve():
    return itofin.FlatForward(
        itofin.Date(*SETTLEMENT), CURVE_RATE, itofin.DayCounter.actual365_fixed()
    )


def _build_helpers(curve, index):
    fixed_dc = itofin.DayCounter.thirty360_bond_basis()
    float_dc = itofin.DayCounter.actual360()
    fixed_tenor = itofin.Period(1, "Years")
    helpers = []
    for maturity, length, vol in SWAPTIONS:
        helpers.append(
            itofin.SwaptionHelper(
                itofin.Period(maturity, "Years"),
                itofin.Period(length, "Years"),
                vol,
                index,
                fixed_tenor,
                fixed_dc,
                float_dc,
                curve,
                itofin.CalibrationErrorType.RelativePriceError,
                1.0,
            )
        )
    return helpers


def test_hullwhite_calibrates_to_the_cached_swaption_values():
    settings = _fixture_settings()
    curve = _fixture_curve()
    model = itofin.HullWhite(curve, 0.1, 0.01)
    index = itofin.Euribor.six_months(curve, settings)
    helpers = _build_helpers(curve, index)

    method = itofin.LevenbergMarquardt(1e-8, 1e-8, 1e-8, False)
    end_criteria = itofin.EndCriteria(10000, 100, 1e-6, 1e-8, 1e-8)
    model.calibrate(helpers, method, end_criteria, False)

    assert model.a() == pytest.approx(0.0464041, abs=1.3e-5)
    assert model.sigma() == pytest.approx(0.00579912, abs=1.3e-5)

    # A 2-parameter fit to 5 swaptions is not exact; the per-helper relative-price
    # errors are NOT tiny here (the RSS residual is ~0.1158). Pin only that each
    # helper's error is finite, i.e. the engine priced and the fit converged.
    for helper in helpers:
        error = helper.calibration_error()
        assert error == error
        assert abs(error) < float("inf")


def test_hullwhite_calibrates_with_fixed_reversion():
    # Optional second arm (shortratemodels.cpp:155-227, hullwhite.rs:1036-1044):
    # pin the mean reversion a at 0.05 and free only sigma. This arm uses a
    # DIFFERENT EndCriteria(1000, 500, ...) than the par arm; a must stay exactly
    # 0.05 (the fixed parameter must not move), and sigma fits to 0.00585858.
    settings = _fixture_settings()
    curve = _fixture_curve()
    model = itofin.HullWhite(curve, 0.05, 0.01)
    index = itofin.Euribor.six_months(curve, settings)
    helpers = _build_helpers(curve, index)

    method = itofin.LevenbergMarquardt(1e-8, 1e-8, 1e-8, False)
    end_criteria = itofin.EndCriteria(1000, 500, 1e-8, 1e-8, 1e-8)
    model.calibrate(helpers, method, end_criteria, True)

    assert model.a() == pytest.approx(0.05, abs=1e-15)
    assert model.sigma() == pytest.approx(0.00585858, abs=1e-5)


def test_calibrate_with_no_helpers_raises():
    settings = _fixture_settings()
    curve = _fixture_curve()
    model = itofin.HullWhite(curve, 0.1, 0.01)
    itofin.Euribor.six_months(curve, settings)
    method = itofin.LevenbergMarquardt(1e-8, 1e-8, 1e-8, False)
    end_criteria = itofin.EndCriteria(10000, 100, 1e-6, 1e-8, 1e-8)
    with pytest.raises(itofin.ItofinError):
        model.calibrate([], method, end_criteria, False)
