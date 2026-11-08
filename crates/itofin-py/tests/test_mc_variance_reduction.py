"""brownian_bridge and control_variate on the Python MC engines (#1364).

The pinned values are the C ABI and Go pins for the same fixtures and seeds:
the core #1362 bridge oracle (12 steps, 40000 samples, seed 42) and the core
#1363 control-variate oracle (K=32 antithetic put, 75 steps, 4096 samples,
seed 42, polynomial order 3).
"""

import pytest

from itofin import Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.pricingengines import (
    MCAmericanEngine,
    MCEuropeanEngine,
    MCEuropeanHestonEngine,
    QMCEuropeanEngine,
)
from itofin.processes import BlackScholesProcess, HestonProcess
from itofin.time import Date, DayCounter

BRIDGE_NPV = 9.271732701042982
CV_NPV = 0.7231366070567805
TODAY = Date(15, 6, 2026)
SETTLEMENT = Date(17, 5, 1998)
MATURITY = Date(17, 5, 1999)


def _european():
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    process = BlackScholesProcess(100.0, 0.05, 0.02, 0.2, TODAY, DayCounter.actual360())
    return VanillaOption(OptionType.Call, 100.0, TODAY + 360, settings), process


def _bridged(process, steps=12, brownian_bridge=True):
    return MCEuropeanEngine(
        process, steps=steps, samples=40000, seed=42, brownian_bridge=brownian_bridge
    )


def test_bridge_reproduces_the_core_oracle():
    option, process = _european()
    analytic = option.price(process)
    bridged = option.price_mc(_bridged(process))
    error = option.error_estimate()
    assert bridged == BRIDGE_NPV
    assert 0 < error and abs(bridged - analytic) < 3 * error
    assert option.price_mc(_bridged(process, brownian_bridge=False)) != bridged
    one = option.price_mc(_bridged(process, steps=1))
    assert one == pytest.approx(
        option.price_mc(_bridged(process, steps=1, brownian_bridge=False)), rel=1e-12
    )
    default = MCEuropeanEngine(process, steps=12, samples=40000, seed=42)
    assert option.price_mc(default) == option.price_mc(_bridged(process, brownian_bridge=False))


def test_qmc_bridge_reprices_the_one_step_run():
    option, process = _european()

    def qmc(steps, brownian_bridge, seed=0):
        engine = QMCEuropeanEngine(
            process, steps=steps, samples=4095, seed=seed, brownian_bridge=brownian_bridge
        )
        return option.price_qmc(engine)

    bridged = qmc(40, True)
    assert bridged == pytest.approx(qmc(1, False), abs=1e-9)
    assert qmc(40, True, seed=7) == pytest.approx(bridged, abs=1e-9)
    assert abs(qmc(40, False) - bridged) > 1e-9


def _american(control_variate):
    settings = Settings()
    settings.set_evaluation_date(Date(15, 5, 1998))
    process = BlackScholesProcess(36.0, 0.06, 0.0, 0.2, SETTLEMENT, DayCounter.actual365_fixed())
    option = VanillaOption.american(
        OptionType.Put, 32.0, earliest=SETTLEMENT, latest=MATURITY, settings=settings
    )
    engine = MCAmericanEngine(
        process,
        steps=75,
        samples=4096,
        seed=42,
        antithetic=True,
        polynomial_order=3,
        control_variate=control_variate,
    )
    return option, engine


def test_control_variate_reproduces_the_core_oracle():
    option, engine = _american(True)
    value = option.price_mc_american(engine)
    error, probability = option.error_estimate(), option.exercise_probability()
    assert value == CV_NPV
    plain_option, plain_engine = _american(False)
    plain = plain_option.price_mc_american(plain_engine)
    assert plain != value
    assert error < plain_option.error_estimate()
    assert probability == plain_option.exercise_probability()
    default_option, default_engine = _american(None)
    assert default_option.price_mc_american(default_engine) == plain


def test_unsupported_combinations_are_not_accepted():
    _, process = _european()
    heston = HestonProcess(0.05, 0.0, 100.0, 0.04, 1.0, 0.04, 0.3, -0.5, TODAY, DayCounter.actual360())
    with pytest.raises(TypeError, match="control_variate"):
        MCEuropeanEngine(process, steps=1, samples=10, control_variate=True)  # type: ignore[call-arg]
    with pytest.raises(TypeError, match="control_variate"):
        QMCEuropeanEngine(process, steps=1, samples=4095, control_variate=True)  # type: ignore[call-arg]
    with pytest.raises(TypeError, match="brownian_bridge"):
        MCAmericanEngine(process, steps=1, samples=10, brownian_bridge=True)  # type: ignore[call-arg]
    with pytest.raises(TypeError, match="brownian_bridge"):
        MCEuropeanHestonEngine(heston, steps=1, samples=10, brownian_bridge=True)  # type: ignore[call-arg]
