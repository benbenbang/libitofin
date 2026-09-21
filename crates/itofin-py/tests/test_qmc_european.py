"""QuantLib europeanoption.cpp testQmcEngines: all 108 cases, unchanged band."""

import gc
import itertools
import math

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.pricingengines import QMCEuropeanEngine
from itofin.processes import BlackScholesProcess
from itofin.quotes import SimpleQuote
from itofin.termstructures import BlackConstantVol, FlatForward
from itofin.time import Date, DayCounter

TODAY = Date(15, 6, 2026)
EXPIRY = TODAY + 360


def test_qmc_quantlib_108_cases():
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    for kind, strike, dividend, rate, vol in itertools.product(
        [OptionType.Call, OptionType.Put],
        [75.0, 100.0, 125.0],
        [0.0, 0.05],
        [0.01, 0.05, 0.15],
        [0.11, 0.50, 1.20],
    ):
        process = BlackScholesProcess(100.0, rate, dividend, vol, TODAY, DayCounter.actual360())
        option = VanillaOption(kind, strike, EXPIRY, settings)
        analytic = option.price(process)
        engine = QMCEuropeanEngine(process, steps=1, samples=4095)
        qmc = option.price_qmc(engine)
        assert math.isfinite(qmc)
        assert abs(qmc - analytic) / 100.0 <= 0.01
        assert option.price_qmc(QMCEuropeanEngine(process, steps=1, samples=4095)) == qmc
        with pytest.raises(ItofinError, match="error estimate not provided"):
            option.error_estimate()


def test_qmc_antithetic_and_explicit_zero_seed():
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    process = BlackScholesProcess(100.0, 0.05, 0.02, 0.2, TODAY, DayCounter.actual360())
    option = VanillaOption(OptionType.Call, 100.0, EXPIRY, settings)
    implicit = option.price_qmc(QMCEuropeanEngine(process, steps=1, samples=4095))
    assert option.price_qmc(QMCEuropeanEngine(process, steps=1, samples=4095, seed=0)) == implicit
    antithetic = option.price_qmc(QMCEuropeanEngine(process, steps=1, samples=4095, antithetic=True))
    assert math.isfinite(antithetic)
    assert abs(antithetic - option.price(process)) / 100.0 <= 0.01


def test_qmc_live_quote_retention_and_cold_calculation():
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    dc = DayCounter.actual360()
    quote = SimpleQuote(0.05)
    rate = FlatForward.from_quote(TODAY, quote, dc)
    dividend = FlatForward(TODAY, 0.02, dc)
    vol = BlackConstantVol(TODAY, 0.2, dc)
    process = BlackScholesProcess.from_curves(100.0, rate, dividend, vol)
    engine = QMCEuropeanEngine(process, steps=1, samples=4095)
    option = VanillaOption(OptionType.Call, 100.0, EXPIRY, settings)
    option.set_qmc_engine(engine)
    del engine, process, rate, dividend, vol, dc, settings
    gc.collect()
    initial = option.npv()
    quote.set_value(0.08)
    updated = option.npv()
    assert math.isfinite(initial) and math.isfinite(updated)
    assert updated != initial
    fresh_settings = Settings()
    fresh_settings.set_evaluation_date(TODAY)
    fresh = VanillaOption(OptionType.Call, 100.0, EXPIRY, fresh_settings)
    fresh_process = BlackScholesProcess(100.0, 0.08, 0.02, 0.2, TODAY, DayCounter.actual360())
    assert updated == fresh.price_qmc(QMCEuropeanEngine(fresh_process, steps=1, samples=4095))
    quote.set_value(0.05)
    del quote
    gc.collect()
    assert option.npv() == initial


@pytest.mark.parametrize(
    "config",
    [
        {"steps": 1, "samples": 2**32},
        {"samples": 4095},
        {"steps": 1},
        {"steps": 0, "samples": 4095},
        {"steps_per_year": 0, "samples": 4095},
        {"steps": 1, "samples": 0},
        {"steps": 1, "steps_per_year": 1, "samples": 4095},
        {"steps": 1, "absolute_tolerance": 0.01},
        {"steps": 1, "samples": 4095, "absolute_tolerance": 0.01},
        {"steps": 1, "samples": 4095, "max_samples": 8191},
    ],
)
def test_qmc_invalid_configuration_recovers(config):
    process = BlackScholesProcess(100.0, 0.05, 0.02, 0.2, TODAY, DayCounter.actual360())
    with pytest.raises(ItofinError):
        QMCEuropeanEngine(process, **config)
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    option = VanillaOption(OptionType.Call, 100.0, EXPIRY, settings)
    assert math.isfinite(option.price_qmc(QMCEuropeanEngine(process, steps_per_year=1, samples=4095)))
