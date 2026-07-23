import math

import pytest

from itofin import ItofinError, Settings
from itofin.models import CalibrationErrorType, HestonModel, HestonModelHelper
from itofin.optimization import EndCriteria, LevenbergMarquardt
from itofin.processes import HestonProcess
from itofin.termstructures import FlatForward
from itofin.time import Calendar, Date, DayCounter, Period

VOL = 0.1
EXPECTED_VARIANCE = VOL * VOL
REFERENCE = (15, 1, 2026)

# The seven maturity year-fractions, derived exactly as the core helper derives
# them: tau = risk_free.time_from_reference(calendar.advance_by_period(reference,
# maturity, Following, false)) over the Actual360 flat curve referenced at
# Date(15, 1, 2026) on a NullCalendar (hestonmodelhelper.rs:345-355). The Python
# facade exposes no calendar-advance or month arithmetic, so these are pinned as
# constants produced by a throwaway probe over the exact core code path (the
# Fixture::tau helper, printed to 15 places and reverted, not committed). The
# maturities are {1, 2, 3, 6, 9 Months, 1, 2 Years}.
TAUS = [
    0.086111111111111,
    0.163888888888889,
    0.250000000000000,
    0.502777777777778,
    0.758333333333333,
    1.013888888888889,
    2.027777777777778,
]
MATURITIES = [
    (1, "Months"),
    (2, "Months"),
    (3, "Months"),
    (6, "Months"),
    (9, "Months"),
    (1, "Years"),
    (2, "Years"),
]
MONEYNESSES = [-1.0, 0.0, 1.0]


def _fixture_settings():
    settings = Settings()
    settings.set_evaluation_date(Date(*REFERENCE))
    return settings


def _build_helpers(settings):
    # The testBlackCalibration market (hestonmodel.cpp:239-251): Actual360 flat
    # curves at 4% risk-free / 50% dividend, unit spot, flat 10% vol, on a
    # NullCalendar with RelativePriceError. Strikes follow
    # fwd * exp(-moneyness * VOL * sqrt(tau)) with fwd = s0 * div_discount(tau)
    # / rf_discount(tau) (hestonmodelhelper.rs:357-363, :519-521); the two
    # discounts are read from FlatForward curves that match the helper's own
    # curves exactly, so the mechanism stays live rather than pasting opaque
    # strike constants.
    ref = Date(*REFERENCE)
    dc = DayCounter.actual360()
    calendar = Calendar.null_calendar()
    risk_free = FlatForward(ref, 0.04, dc)
    dividend = FlatForward(ref, 0.50, dc)

    helpers = []
    for (n, unit), tau in zip(MATURITIES, TAUS):
        forward = 1.0 * dividend.discount(tau) / risk_free.discount(tau)
        for moneyness in MONEYNESSES:
            strike = forward * math.exp(-moneyness * VOL * math.sqrt(tau))
            helpers.append(
                HestonModelHelper(
                    Period(n, unit),
                    calendar,
                    1.0,
                    strike,
                    VOL,
                    0.04,
                    0.50,
                    CalibrationErrorType.RelativePriceError,
                    ref,
                    dc,
                    settings,
                )
            )
    return helpers


def _seed_model():
    # A fresh model seeded from a HestonProcess (v0=0.01, kappa=0.2, theta=0.02,
    # sigma, rho=-0.75) on the same Actual360 today() flat curves as the helpers
    # (hestonmodelhelper.rs:545-555). The process facade builds those curves
    # internally with the identical reference date and day counter, so the
    # engine and the helpers price on the same market.
    def build(sigma):
        process = HestonProcess(
            0.04,
            0.50,
            1.0,
            0.01,
            0.2,
            0.02,
            sigma,
            -0.75,
            Date(*REFERENCE),
            DayCounter.actual360(),
        )
        return HestonModel(process)

    return build


def test_heston_calibrates_to_a_flat_vol_surface():
    # ORACLE testBlackCalibration (hestonmodel.cpp:232-311,
    # hestonmodelhelper.rs:501-600): calibrate to a flat 10% vol surface over 21
    # helpers. A flat surface has no smile, so the fit drives sigma to zero and
    # theta/v0 to the constant variance. Tolerance 3e-3; the theta pin is C++'s
    # deliberately weak product |kappa * (theta - vol^2)|, ported faithfully (not
    # "fixed" to |theta - v0|). The Rust oracle loops three seed vol-of-vols
    # {0.1, 0.3, 0.5}; all three are reproduced here.
    settings = _fixture_settings()
    helpers = _build_helpers(settings)
    assert len(helpers) == 21
    build = _seed_model()

    for sigma in (0.1, 0.3, 0.5):
        model = build(sigma)
        method = LevenbergMarquardt(1e-8, 1e-8, 1e-8, False)
        end_criteria = EndCriteria(400, 40, 1e-8, 1e-8, 1e-8)
        model.calibrate(helpers, method, end_criteria, 96)

        assert model.sigma() < 3e-3, f"sigma {model.sigma()} (start {sigma})"
        theta_residual = model.kappa() * (model.theta() - EXPECTED_VARIANCE)
        assert abs(theta_residual) < 3e-3, f"kappa*(theta-vol^2) {theta_residual} (start {sigma})"
        assert abs(model.v0() - EXPECTED_VARIANCE) < 3e-3, f"v0 {model.v0()} (start {sigma})"

        for helper in helpers:
            assert helper.calibration_error() < 1e-2


def test_calibrate_with_no_helpers_raises():
    model = _seed_model()(0.5)
    method = LevenbergMarquardt(1e-8, 1e-8, 1e-8, False)
    end_criteria = EndCriteria(400, 40, 1e-8, 1e-8, 1e-8)
    with pytest.raises(ItofinError):
        model.calibrate([], method, end_criteria, 96)
