"""Original QuantLib DAX prices and 21-instrument calibration acceptance.

The model retains the original nonflat zero curve. Existing Python Heston
helpers accept scalar rates rather than curve handles, so each helper uses
its exact TARGET-week expiry's continuously compounded zero rate. This is an
expiry-discount adapter, not a claim that helpers expose yield-curve handles.
"""

import math

import pytest

from itofin import Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.models import CalibrationErrorType, GJRGARCHModel, HestonModelHelper
from itofin.optimization import EndCriteria, Simplex
from itofin.pricingengines import AnalyticGJRGARCHEngine
from itofin.processes import GJRGARCHProcess
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward, ZeroCurve
from itofin.time import Calendar, Date, DayCounter, Period
from test_gjr_model_oracle import fixture

DAX = fixture("dax")
REFERENCE = Date(5, 7, 2002)


def setup(params):
    """Retain the original nonflat curve and requested model parameter vector."""
    settings = Settings()
    settings.set_evaluation_date(REFERENCE)
    dc = DayCounter.actual365_fixed()
    risk_free = ZeroCurve([REFERENCE + days for days in DAX["zero_curve_days"]], DAX["zero_curve_rates"], dc)
    dividend = FlatForward(REFERENCE, 0.0, dc)
    omega, alpha, beta, gamma, lambda_, v0 = params
    process = GJRGARCHProcess(
        SimpleQuote(DAX["spot"]), risk_free, dividend, v0, omega, alpha, beta, gamma, lambda_, DAX["days_per_year"]
    )
    return settings, dc, risk_free, GJRGARCHModel(process)


@pytest.mark.parametrize("row", DAX["rows"])
def test_original_dax_fitted_parameter_prices_match_native_fixture(row):
    """Known native fitted parameters reproduce all 21 original model prices."""
    settings, _, risk_free, model = setup(DAX["calibrated_params"])
    maturity = REFERENCE + row["maturity_days"]
    assert risk_free.discount_date(maturity) == pytest.approx(row["risk_free_discount"], rel=2e-13, abs=1e-14)
    kind = OptionType.Call if row["option_type"] == 1 else OptionType.Put
    instrument = VanillaOption(kind, row["strike"], maturity, settings)
    assert instrument.price_gjr(AnalyticGJRGARCHEngine(model)) == pytest.approx(row["model_price"], rel=2e-12, abs=1e-8)


def test_original_21_helper_dax_calibration_meets_upstream_acceptance():
    """Preserve the upstream percent-volatility SSE <= 15 acceptance band."""
    settings, dc, risk_free, model = setup(DAX["initial_params"])
    helpers = []
    calendar = Calendar.target()
    for row in DAX["rows"]:
        maturity = REFERENCE + row["maturity_days"]
        years = row["maturity_days"] / 365.0
        rate = -math.log(risk_free.discount_date(maturity)) / years
        helpers.append(
            HestonModelHelper(
                Period(row["weeks"], "Weeks"),
                calendar,
                DAX["spot"],
                row["strike"],
                row["market_vol"],
                rate,
                0.0,
                CalibrationErrorType.ImpliedVolError,
                REFERENCE,
                dc,
                settings,
            )
        )
    model.calibrate(
        helpers,
        Simplex(DAX["simplex_lambda"]),
        EndCriteria(DAX["max_iterations"], DAX["stationary_iterations"], *DAX["tolerances"]),
    )
    errors = [helper.calibration_error() for helper in helpers]
    assert all(math.isfinite(error) for error in errors)
    assert sum((100.0 * error) ** 2 for error in errors) <= 15.0
    assert all(math.isfinite(value) for value in model.params())
