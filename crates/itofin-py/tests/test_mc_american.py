"""Oracle for the Python MCAmericanEngine facade and the American VanillaOption.

Mirrors the core oracle in
crates/libitofin/src/pricingengines/vanilla/mcamericanengine.rs:762-812, itself
a port of test-suite/americanoption.cpp: the 36-strike American put on a flat
Actual365Fixed market, priced with withSteps(75).withAntitheticVariate(true)
.withAbsoluteTolerance(0.02).withSeed(42).withPolynomialOrder(3). The reference
2.054422273006143 is the value a locally-built QuantLib 1.43 produces for this
exact configuration, banded by the engine's own error estimate.
"""

# pypi/conda library
import pytest

# itofin library
from itofin import ItofinError, Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.pricingengines import MCAmericanEngine
from itofin.processes import BlackScholesProcess
from itofin.time import Date, DayCounter

TODAY = Date(15, 5, 1998)
SETTLEMENT = Date(17, 5, 1998)
MATURITY = Date(17, 5, 1999)

UNDERLYING = 36.0
STRIKE = 36.0
RISK_FREE_RATE = 0.06
DIVIDEND_YIELD = 0.0
VOLATILITY = 0.20

QUANTLIB_MC_VALUE = 2.054422273006143
EXPECTED_EXERCISE_PROBABILITY = 0.48013


def _market():
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    process = BlackScholesProcess(
        UNDERLYING,
        RISK_FREE_RATE,
        DIVIDEND_YIELD,
        VOLATILITY,
        SETTLEMENT,
        DayCounter.actual365_fixed(),
    )
    return settings, process


def _american_option(settings):
    return VanillaOption.american(
        OptionType.Put, STRIKE, earliest=SETTLEMENT, latest=MATURITY, settings=settings
    )


def _milestone_engine(process, seed=42, antithetic=True):
    return MCAmericanEngine(
        process,
        steps=75,
        antithetic=antithetic,
        absolute_tolerance=0.02,
        seed=seed,
        polynomial_order=3,
    )


def test_american_put_reproduces_the_quantlib_price_and_exercise_probability():
    settings, process = _market()
    option = _american_option(settings)
    option.set_mc_american_engine(_milestone_engine(process))

    npv = option.npv()
    error_estimate = option.error_estimate()
    exercise_probability = option.exercise_probability()

    european = VanillaOption(OptionType.Put, STRIKE, MATURITY, settings)
    european.set_engine(process)
    european_npv = european.npv()

    assert abs(npv - QUANTLIB_MC_VALUE) < 2.34 * error_estimate
    assert abs(exercise_probability - EXPECTED_EXERCISE_PROBABILITY) < 0.015
    assert npv > european_npv


def test_european_exercise_is_rejected_by_the_american_engine():
    settings, process = _market()
    option = VanillaOption(OptionType.Put, STRIKE, MATURITY, settings)
    option.set_mc_american_engine(_milestone_engine(process))

    with pytest.raises(ItofinError, match="wrong exercise given"):
        option.npv()


def test_exercise_probability_is_unavailable_on_the_analytic_engine():
    settings, process = _market()
    option = VanillaOption(OptionType.Put, STRIKE, MATURITY, settings)
    option.set_engine(process)

    with pytest.raises(ItofinError, match="exerciseProbability"):
        option.exercise_probability()


def _fixed_sample_npv(
    seed=42, antithetic=True, polynomial_order=3, calibration_samples=None
):
    settings, process = _market()
    option = _american_option(settings)
    option.set_mc_american_engine(
        MCAmericanEngine(
            process,
            steps=75,
            antithetic=antithetic,
            samples=2048,
            seed=seed,
            polynomial_order=polynomial_order,
            calibration_samples=calibration_samples,
        )
    )
    return option.npv()


def test_same_seed_reproduces_bitwise_and_another_seed_differs():
    first = _fixed_sample_npv(42, True)
    second = _fixed_sample_npv(42, True)
    other = _fixed_sample_npv(43, True)

    assert first == second
    assert first != other


def test_antithetic_flag_reaches_the_core():
    assert _fixed_sample_npv(42, True) != _fixed_sample_npv(42, False)


def test_regression_settings_reach_the_core():
    baseline = _fixed_sample_npv()

    assert baseline != _fixed_sample_npv(polynomial_order=4)
    assert baseline != _fixed_sample_npv(calibration_samples=1024)
