import math

import itofin
import pytest


def test_period_builds_with_known_unit():
    p = itofin.Period(6, "Months")
    assert repr(p) == "Period(6, Months)"


def test_period_rejects_unknown_unit():
    with pytest.raises(itofin.ItofinError):
        itofin.Period(1, "Fortnights")


def test_levenberg_marquardt_builds_with_defaults():
    itofin.LevenbergMarquardt()


def test_levenberg_marquardt_accepts_explicit_args():
    itofin.LevenbergMarquardt(1e-10, 1e-10, 1e-10, True)


def test_end_criteria_builds():
    itofin.EndCriteria(400, 40, 1e-8, 1e-8, 1e-8)


def test_end_criteria_rejects_stationary_ge_iterations():
    with pytest.raises(itofin.ItofinError):
        itofin.EndCriteria(400, 500, 1e-8, 1e-8, 1e-8)


def test_end_criteria_rejects_stationary_not_gt_one():
    with pytest.raises(itofin.ItofinError):
        itofin.EndCriteria(400, 1, 1e-8, 1e-8, 1e-8)


def test_end_criteria_requires_all_five_arguments():
    with pytest.raises(TypeError):
        itofin.EndCriteria(400)
    with pytest.raises(TypeError):
        itofin.EndCriteria(400, 40, 1e-8, 1e-8)


def test_flat_forward_discount_matches_continuous_flat():
    curve = itofin.FlatForward(
        itofin.Date(15, 1, 2026), 0.03, itofin.DayCounter.actual365_fixed()
    )
    assert curve.discount(1.0) == pytest.approx(math.exp(-0.03 * 1.0), abs=1e-12)
    assert curve.discount(0.0) == pytest.approx(1.0, abs=1e-12)


def test_flat_forward_zero_rate_is_flat():
    curve = itofin.FlatForward(
        itofin.Date(15, 1, 2026), 0.03, itofin.DayCounter.actual365_fixed()
    )
    assert curve.zero_rate(1.0) == pytest.approx(0.03, abs=1e-12)


def test_calibration_error_types_are_constructible():
    assert itofin.CalibrationErrorType.RelativePriceError is not None
    assert itofin.CalibrationErrorType.PriceError is not None
    assert itofin.CalibrationErrorType.ImpliedVolError is not None
    assert (
        itofin.CalibrationErrorType.RelativePriceError
        != itofin.CalibrationErrorType.PriceError
    )
