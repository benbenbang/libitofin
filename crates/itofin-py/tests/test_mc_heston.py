"""QuantLib-cached oracle for the Python MCEuropeanHestonEngine facade.

Mirrors the core oracle in
crates/libitofin/src/pricingengines/vanilla/mceuropeanhestonengine.rs:704-738,
itself a port of test-suite/hestonmodel.cpp:536-589 testMcVsCached: a Put(1.05)
struck at the spot, settlement 27-Dec-2004 / exercise 28-Mar-2005 on
ActualActual(ISDA), priced with stepsPerYear 11, the antithetic variate, 50000
samples and seed 1234.

The 0.0632851308977151 literal is a documented stale-upstream artifact - current
C++ QuantLib misses it by the same 0.29 * error_estimate - so the gate is
QuantLib's own 2.34 * error_estimate band, not a bit-exact tolerance. Pricing is
seeded, so this facade reproduces the core's 0.0632327393488322 bitwise.
"""

from itofin import Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.pricingengines import MCEuropeanHestonEngine
from itofin.processes import HestonProcess
from itofin.time import Date, DayCounter

SETTLEMENT = Date(27, 12, 2004)
EXERCISE = Date(28, 3, 2005)
CACHED = 0.0632851308977151


def _process():
    return HestonProcess(
        0.7,
        0.4,
        1.05,
        0.3,
        1.16,
        0.2,
        0.8,
        0.8,
        SETTLEMENT,
        DayCounter.actual_actual_isda(),
    )


def _option(samples, seed, antithetic=True):
    settings = Settings()
    settings.set_evaluation_date(SETTLEMENT)
    option = VanillaOption(OptionType.Put, 1.05, EXERCISE, settings)
    option.set_mc_heston_engine(
        MCEuropeanHestonEngine(
            _process(),
            steps_per_year=11,
            antithetic=antithetic,
            samples=samples,
            seed=seed,
        )
    )
    return option


def test_mc_vs_cached():
    option = _option(50000, 1234)
    npv = option.npv()
    se = option.error_estimate()

    assert se > 0.0
    assert se <= 7.5e-4
    assert abs(npv - CACHED) <= 2.34 * se


def test_same_seed_reproduces_bitwise_and_another_seed_differs():
    first = _option(5000, 1234).npv()
    second = _option(5000, 1234).npv()
    other = _option(5000, 1235).npv()

    assert first == second
    assert first != other


def test_antithetic_flag_reaches_the_core():
    antithetic = _option(5000, 1234, antithetic=True).npv()
    plain = _option(5000, 1234, antithetic=False).npv()

    assert antithetic != plain
