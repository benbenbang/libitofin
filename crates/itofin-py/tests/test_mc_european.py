"""MC-vs-analytic oracle for the Python MCEuropeanEngine facade.

Mirrors the core oracle in
crates/libitofin/src/pricingengines/vanilla/mceuropeanengine.rs:435-484, itself
a port of test-suite/europeanoption.cpp:1269 testMcEngines: a European call on
the flat Actual360 market, priced with withSteps(1).withSamples(40000)
.withSeed(42) and checked against the AnalyticEuropeanEngine. The band is the
core's strengthened |mc - analytic| < 3 * error_estimate() convergence pin, not
QuantLib's loose relative band.
"""

# pypi/conda library
import pytest

# itofin library
from itofin import ItofinError, Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.pricingengines import MCEuropeanEngine
from itofin.processes import BlackScholesProcess
from itofin.time import Date, DayCounter

TODAY = Date(15, 6, 2026)
EXPIRY = TODAY + 360
STRIKES = [90.0, 100.0, 110.0]


def _market():
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    process = BlackScholesProcess(100.0, 0.05, 0.02, 0.20, TODAY, DayCounter.actual360())
    return settings, process


def _mc_option(settings, process, strike, seed):
    option = VanillaOption(OptionType.Call, strike, EXPIRY, settings)
    option.set_mc_engine(MCEuropeanEngine(process, steps=1, samples=40000, seed=seed))
    return option


def _analytic_option(settings, process, strike):
    option = VanillaOption(OptionType.Call, strike, EXPIRY, settings)
    option.set_engine(process)
    return option


def test_mc_matches_analytic_across_moneyness():
    settings, process = _market()
    for strike in STRIKES:
        analytic = _analytic_option(settings, process, strike).npv()
        option = _mc_option(settings, process, strike, 42)
        npv = option.npv()
        se = option.error_estimate()

        assert se > 0.0
        assert abs(npv - analytic) < 3.0 * se


def test_same_seed_reproduces_bitwise_and_another_seed_differs():
    settings, process = _market()
    first = _mc_option(settings, process, 100.0, 42).npv()
    second = _mc_option(settings, process, 100.0, 42).npv()
    other = _mc_option(settings, process, 100.0, 43).npv()

    assert first == second
    assert first != other


def test_antithetic_variate_is_rejected():
    _settings, process = _market()
    with pytest.raises(ItofinError, match="antithetic variate not yet supported"):
        MCEuropeanEngine(process, steps=1, samples=1000, antithetic=True)


def test_missing_and_overspecified_steps_are_rejected():
    _settings, process = _market()
    with pytest.raises(ItofinError, match="number of steps not given"):
        MCEuropeanEngine(process, samples=1000)
    with pytest.raises(ItofinError, match="number of steps overspecified"):
        MCEuropeanEngine(process, steps=1, steps_per_year=12, samples=1000)


def test_samples_and_tolerance_are_mutually_exclusive():
    _settings, process = _market()
    with pytest.raises(ItofinError, match="number of samples already set"):
        MCEuropeanEngine(process, steps=1, samples=1000, absolute_tolerance=0.05)


def test_analytic_engine_provides_no_error_estimate():
    settings, process = _market()
    option = _analytic_option(settings, process, 100.0)
    option.npv()
    with pytest.raises(ItofinError, match="error estimate not provided"):
        option.error_estimate()
