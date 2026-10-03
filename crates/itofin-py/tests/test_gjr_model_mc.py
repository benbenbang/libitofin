"""Seeded GJR Monte Carlo configuration, errors and observable lifecycle."""

import gc
import math
from typing import Any

import pytest

from itofin import ItofinError
from itofin.instruments import BermudanExercise, OptionType, VanillaOption
from itofin.pricingengines import MCEuropeanGJRGARCHEngine
from test_gjr_model import EXPIRY, MARKET, REFERENCE, market


def priced(scheme="FullTruncation", seed=1234, antithetic=True, samples=4096):
    """Install a retained seeded engine on the standard European option."""
    setup = market(scheme=scheme)
    process, option = setup[3], setup[-1]
    engine = MCEuropeanGJRGARCHEngine(process, steps=24, samples=samples, seed=seed, antithetic=antithetic)
    option.set_mc_gjr_engine(engine)
    return setup, engine


@pytest.mark.parametrize("scheme", ["PartialTruncation", "FullTruncation", "Reflection"])
def test_all_schemes_expose_seeded_values_and_standard_errors(scheme):
    """MC uses the original process scheme without model regeneration."""
    setup, engine = priced(scheme)
    option = setup[-1]
    value, error = option.npv(), option.error_estimate()
    assert math.isfinite(value)
    assert value > 0.0
    assert math.isfinite(error)
    assert error > 0.0
    assert option.price_mc_gjr(engine) == value
    repeated, _ = priced(scheme)
    assert repeated[-1].npv() == value
    assert repeated[-1].error_estimate() == error


def test_seed_and_antithetic_options_reach_native_engine():
    """Repeatable nonzero seeds and antithetic draws affect the estimates."""
    base, _ = priced()
    other, _ = priced(seed=1235)
    plain, _ = priced(antithetic=False)
    assert base[-1].npv() != other[-1].npv()
    assert base[-1].npv() != plain[-1].npv()


@pytest.mark.parametrize("index,value", list(enumerate((105.0, 0.05, 0.02))))
def test_mc_market_notifications_and_collection_preserve_live_pricing(index, value):
    """The retained MC process continues to observe each market quote."""
    setup, engine = priced(samples=2048)
    settings, quotes, curves, process, model, analytic, option = setup
    before = option.npv()
    quote = quotes[index]
    del setup, settings, quotes, curves, process, model, analytic, engine
    gc.collect()
    quote.set_value(value)
    assert not option.is_calculated()
    values = list(MARKET)
    values[index] = value
    rebuilt = market(values=values)
    rebuilt[-1].set_mc_gjr_engine(
        MCEuropeanGJRGARCHEngine(rebuilt[3], steps=24, samples=2048, seed=1234, antithetic=True)
    )
    assert option.npv() != before
    assert option.npv() == rebuilt[-1].npv()
    assert option.error_estimate() == rebuilt[-1].error_estimate()


def test_steps_per_year_matches_explicit_steps_for_one_year():
    """The time-grid alternatives share the same one-year path configuration."""
    setup, _ = priced()
    reference = setup[-1].npv()
    engine = MCEuropeanGJRGARCHEngine(setup[3], steps_per_year=24, samples=4096, seed=1234, antithetic=True)
    setup[-1].set_mc_gjr_engine(engine)
    assert setup[-1].npv() == reference


def test_tolerance_sampling_returns_a_valid_standard_error():
    """Tolerance-driven sampling is supported with an explicit finite cap."""
    setup = market()
    setup[-1].set_mc_gjr_engine(
        MCEuropeanGJRGARCHEngine(
            setup[3], steps=12, absolute_tolerance=0.5, max_samples=4096, seed=1234, antithetic=True
        )
    )
    assert math.isfinite(setup[-1].npv())
    assert 0.0 < setup[-1].error_estimate() <= 0.5


INVALID: tuple[dict[str, Any], ...] = (
    {},
    {"samples": 2048},
    {"steps": 12},
    {"steps": 12, "steps_per_year": 12, "samples": 2048},
    {"steps": 12, "samples": 2048, "absolute_tolerance": 0.1},
    {"steps": 0, "samples": 2048},
    {"steps_per_year": 0, "samples": 2048},
    {"steps": 12, "samples": 0},
    {"steps": 12, "samples": 1},
    *({"steps": 12, "absolute_tolerance": value} for value in (0.0, -0.1, math.nan, math.inf)),
    {"steps": 12, "absolute_tolerance": 0.1, "max_samples": 1},
    {"steps": 100001, "samples": 2},
    {"steps": 12, "samples": 1000001},
)


@pytest.mark.parametrize("config", INVALID)
def test_native_factory_rejects_invalid_configurations(config):
    """Illegal or unbounded sampling combinations report binding errors."""
    setup = market()
    with pytest.raises(ItofinError):
        MCEuropeanGJRGARCHEngine(setup[3], **config)


@pytest.mark.parametrize("kwargs", [{"brownian_bridge": True}, {"control_variate": True}, {"seed": -1}])
def test_unsupported_features_and_negative_seed_rejected(kwargs):
    """The facade does not silently enable unimplemented sampling policies."""
    setup = market()
    with pytest.raises((TypeError, OverflowError)):
        MCEuropeanGJRGARCHEngine(setup[3], steps=12, samples=2048, **kwargs)


@pytest.mark.parametrize("exercise", ["american", "bermudan"])
def test_mc_rejects_non_european_exercise(exercise):
    """A European terminal payoff engine cannot price early exercise."""
    setup, engine = priced(samples=1024)
    settings = setup[0]
    if exercise == "american":
        option = VanillaOption.american(OptionType.Call, 100.0, REFERENCE, EXPIRY, settings)
    else:
        option = VanillaOption.from_bermudan(
            OptionType.Call, 100.0, BermudanExercise([REFERENCE + 180, EXPIRY]), settings
        )
    option.set_mc_gjr_engine(engine)
    with pytest.raises(ItofinError):
        option.npv()


def test_mc_retains_parameter_snapshot_when_source_model_rebuilds():
    """Model updates do not silently retarget an already built MC engine."""
    setup = market()
    model, option = setup[4], setup[-1]
    snapshot = model.process()
    engine = MCEuropeanGJRGARCHEngine(snapshot, steps=24, samples=2048, seed=1234)
    option.set_mc_gjr_engine(engine)
    before = option.npv()
    updated = model.params()
    updated[-1] *= 1.5
    model.set_params(updated)
    assert snapshot.daily_variance() != model.daily_variance()
    assert option.npv() == before
    option.set_mc_gjr_engine(MCEuropeanGJRGARCHEngine(model.process(), steps=24, samples=2048, seed=1234))
    assert option.npv() != before
