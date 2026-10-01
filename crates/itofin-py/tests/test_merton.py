"""Merton European pricing, live inputs, lifetime and error contracts."""

import gc
import csv
import json
import math
from pathlib import Path
from typing import Any

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import BermudanExercise, OptionType, VanillaOption
from itofin.pricingengines import JumpDiffusionEngine
from itofin.processes import BlackScholesProcess, Merton76Process
from itofin.quotes import SimpleQuote
from itofin.termstructures import BlackConstantVol, FlatForward
from itofin.time import Date, DayCounter

REFERENCE = Date(15, 6, 2026)
EXPIRY = REFERENCE + 360
BASE = (100.0, 0.05, 0.02, 0.2, 0.7, -0.1, 0.25)
GREEKS = ("delta", "gamma", "theta", "vega", "rho", "dividend_rho")


def market(values=BASE, kind=OptionType.Call, accuracy=1e-12, iterations=1000):
    settings = Settings()
    settings.set_evaluation_date(REFERENCE)
    quotes = [SimpleQuote(value) for value in values]
    dc = DayCounter.actual360()
    risk_free = FlatForward.from_quote(REFERENCE, quotes[1], dc)
    dividend = FlatForward.from_quote(REFERENCE, quotes[2], dc)
    volatility = BlackConstantVol.from_quote(REFERENCE, quotes[3], dc)
    process = Merton76Process(quotes[0], risk_free, dividend, volatility, *quotes[4:])
    engine = JumpDiffusionEngine(process, accuracy, iterations)
    option = VanillaOption(kind, 100.0, EXPIRY, settings)
    option.set_jump_diffusion_engine(engine)
    return settings, quotes, process, engine, option


def normal_cdf(x):
    return 0.5 * math.erfc(-x / math.sqrt(2))


def independent_price(values, strike=100.0, t=1.0, kind=OptionType.Call):
    spot, rate, dividend, vol, intensity, log_mean, log_vol = values
    compensation = math.expm1(log_mean + 0.5 * log_vol * log_vol)
    mean = intensity * t
    weight = math.exp(-mean)
    total = 0.0
    for n in range(300):
        variance = vol * vol * t + n * log_vol * log_vol
        log_forward = (
            math.log(spot)
            + (rate - dividend - intensity * compensation) * t
            + n * log_mean
            + 0.5 * n * log_vol * log_vol
        )
        forward = math.exp(log_forward)
        stddev = math.sqrt(variance)
        d1 = (log_forward - math.log(strike) + 0.5 * variance) / stddev
        d2 = d1 - stddev
        if kind == OptionType.Call:
            conditional = forward * normal_cdf(d1) - strike * normal_cdf(d2)
        else:
            conditional = strike * normal_cdf(-d2) - forward * normal_cdf(-d1)
        total += weight * conditional
        weight *= mean / (n + 1)
    return math.exp(-rate * t) * total


@pytest.mark.parametrize("kind", [OptionType.Call, OptionType.Put])
def test_merton_price_independent_poisson_mixture_and_parity(kind):
    _, _, process, engine, option = market(kind=kind)
    expected = independent_price(BASE, kind=kind)
    assert option.npv() == pytest.approx(expected, rel=0, abs=1e-10)
    assert option.price_jump_diffusion(engine) == pytest.approx(expected, rel=0, abs=1e-10)
    assert process.spot() == BASE[0]
    assert process.jump_intensity() == BASE[4]
    assert process.log_mean_jump() == BASE[5]
    assert process.log_jump_volatility() == BASE[6]
    assert process.time(EXPIRY) == 1.0
    assert process.time(REFERENCE) == 0.0


@pytest.mark.parametrize("kind", [OptionType.Call, OptionType.Put])
def test_zero_jump_matches_black_scholes_value_and_all_greeks(kind):
    values = (*BASE[:4], 0.0, *BASE[5:])
    settings, _, _, _, option = market(values, kind)
    diffusion = BlackScholesProcess(*values[:4], REFERENCE, DayCounter.actual360())
    reference = VanillaOption(kind, 100.0, EXPIRY, settings)
    reference.set_engine(diffusion)
    for method in ("npv", *GREEKS):
        assert getattr(option, method)() == pytest.approx(getattr(reference, method)(), rel=0, abs=1e-10)


@pytest.mark.parametrize("index,value", enumerate((105.0, 0.07, 0.04, 0.3, 1.2, -0.2, 0.4)))
def test_all_seven_observable_inputs_invalidate_and_reprice(index, value):
    _, quotes, _, _, option = market()
    before = option.npv()
    assert option.is_calculated()
    quotes[index].set_value(value)
    assert not option.is_calculated()
    updated = list(BASE)
    updated[index] = value
    after = option.npv()
    assert after != before
    assert after == pytest.approx(independent_price(updated), rel=0, abs=1e-10)
    _, _, _, _, rebuilt = market(updated)
    assert after == pytest.approx(rebuilt.npv(), rel=0, abs=1e-12)


def test_option_and_engine_retain_market_after_python_inputs_deleted():
    settings, quotes, process, engine, option = market()
    expected = option.npv()
    retained_quote = quotes[4]
    del quotes, process, engine
    gc.collect()
    retained_quote.set_value(1.1)
    assert not option.is_calculated()
    values = (*BASE[:4], 1.1, *BASE[5:])
    assert option.npv() == pytest.approx(independent_price(values), rel=0, abs=1e-10)
    del settings
    gc.collect()
    retained_quote.set_value(1.3)
    assert not option.is_calculated()
    values = (*BASE[:4], 1.3, *BASE[5:])
    assert option.npv() == pytest.approx(independent_price(values), rel=0, abs=1e-10)
    del retained_quote
    gc.collect()
    assert option.npv() != expected
    assert math.isfinite(option.npv())


@pytest.mark.parametrize(
    "index,bad",
    [
        (0, 0.0),
        (0, -1.0),
        (0, math.nan),
        (0, math.inf),
        (1, math.nan),
        (1, math.inf),
        (2, math.nan),
        (2, math.inf),
        (3, -0.1),
        (3, math.nan),
        (3, math.inf),
        (4, -0.1),
        (4, math.nan),
        (4, math.inf),
        (5, math.nan),
        (5, math.inf),
        (5, 1000.0),
        (5, -1000.0),
        (6, -0.1),
        (6, math.nan),
        (6, math.inf),
        (6, 1e200),
    ],
)
def test_invalid_market_input_rejected_at_pricing_and_recoverable(index, bad):
    _, quotes, _, _, option = market()
    expected = option.npv()
    quotes[index].set_value(bad)
    with pytest.raises(ItofinError):
        option.npv()
    quotes[index].set_value(BASE[index])
    assert option.npv() == pytest.approx(expected, rel=0, abs=1e-12)


@pytest.mark.parametrize(
    "index,bad",
    [
        (4, -0.1),
        (4, math.nan),
        (4, math.inf),
        (5, math.nan),
        (5, math.inf),
        (5, 1000.0),
        (5, -1000.0),
        (6, -0.1),
        (6, math.nan),
        (6, math.inf),
        (6, 1e200),
    ],
)
def test_invalid_jump_quotes_rejected_at_construction(index, bad):
    values = list(BASE)
    values[index] = bad
    with pytest.raises(ItofinError):
        market(values)


@pytest.mark.parametrize(
    "accuracy,iterations", [(0.0, 100), (-0.1, 100), (math.nan, 100), (math.inf, 100), (1e-4, 0), (1e-4, 100001)]
)
def test_invalid_engine_configuration(accuracy, iterations):
    _, _, process, _, _ = market()
    with pytest.raises(ItofinError):
        JumpDiffusionEngine(process, accuracy, iterations)


def test_iteration_limit_is_explicit_failure_and_option_recovers():
    _, _, process, _, option = market()
    option.set_jump_diffusion_engine(JumpDiffusionEngine(process, 1e-12, 1))
    with pytest.raises(ItofinError):
        option.npv()
    option.set_jump_diffusion_engine(JumpDiffusionEngine(process, 1e-12, 1000))
    assert option.npv() == pytest.approx(independent_price(BASE), rel=0, abs=1e-10)


def test_default_engine_configuration_prices_successfully():
    _, _, process, _, option = market()
    option.set_jump_diffusion_engine(JumpDiffusionEngine(process))
    assert option.npv() == pytest.approx(independent_price(BASE), rel=0, abs=1e-3)


@pytest.mark.parametrize("style", ["american", "bermudan"])
def test_only_european_exercise_is_supported(style):
    settings, _, _, engine, _ = market()
    if style == "american":
        option = VanillaOption.american(OptionType.Put, 100.0, REFERENCE, EXPIRY, settings)
    else:
        option = VanillaOption.from_bermudan(
            OptionType.Put, 100.0, BermudanExercise([REFERENCE + 180, EXPIRY]), settings
        )
    option.set_jump_diffusion_engine(engine)
    with pytest.raises(ItofinError):
        option.npv()


def test_wrong_binding_arguments_and_no_gaussian_path_surface():
    _, quotes, process, engine, option = market()
    dynamic_process: Any = Merton76Process
    dynamic_engine: Any = JumpDiffusionEngine
    with pytest.raises(TypeError):
        dynamic_process(*([object()] * 7))
    with pytest.raises(TypeError):
        dynamic_engine(quotes[0])
    with pytest.raises(TypeError):
        dynamic_option: Any = option
        dynamic_option.set_jump_diffusion_engine(process)
    with pytest.raises(TypeError):
        dynamic_engine(process, 1e-4, 1.5)
    with pytest.raises(OverflowError):
        dynamic_engine(process, 1e-4, -1)
    for unsupported in ("drift", "diffusion", "apply", "evolve"):
        assert not hasattr(process, unsupported)
    assert engine is not None


def test_quantlib_merton_value_and_greeks_shared_oracle():
    fixture = json.loads((Path(__file__).resolve().parents[3] / "sdk/go/testdata/merton76-oracle.json").read_text())
    assert len(fixture["cases"]) == 12
    for case in fixture["cases"]:
        inputs = case["input"]
        quotes = [
            SimpleQuote(inputs[key])
            for key in (
                "spot",
                "risk_free_rate",
                "dividend_yield",
                "volatility",
                "jump_intensity",
                "log_mean_jump",
                "log_jump_volatility",
            )
        ]
        settings = Settings()
        settings.set_evaluation_date(REFERENCE)
        clock_variant = inputs["clock_variant"]
        curve_dc = DayCounter.actual365_fixed() if clock_variant == 1 else DayCounter.actual360()
        vol_reference = REFERENCE + (-30 if clock_variant == 1 else 0)
        risk_free = FlatForward.from_quote(REFERENCE, quotes[1], curve_dc)
        dividend = FlatForward.from_quote(REFERENCE, quotes[2], curve_dc)
        volatility = BlackConstantVol.from_quote(vol_reference, quotes[3], DayCounter.actual360())
        process = Merton76Process(quotes[0], risk_free, dividend, volatility, *quotes[4:])
        engine = JumpDiffusionEngine(process, inputs["relative_accuracy"], inputs["max_iterations"])
        kind = OptionType.Call if inputs["option_type"] == "call" else OptionType.Put
        option = VanillaOption(kind, inputs["strike"], REFERENCE + inputs["maturity_days"], settings)
        option.set_jump_diffusion_engine(engine)
        actual = {"value": option.npv(), **{name: getattr(option, name)() for name in GREEKS}}
        assert actual == pytest.approx(case["expected"], rel=0, abs=1e-8), case["name"]


def test_zero_initial_poisson_terms_do_not_hide_later_payoff():
    values = (100.0, 0.05, 0.02, 0.001, 0.01, 1.0, 0.0)
    settings, _, _, engine, _ = market(values)
    option = VanillaOption(OptionType.Call, 1000.0, EXPIRY, settings)
    option.set_jump_diffusion_engine(engine)
    assert option.npv() == pytest.approx(0.00016415889568634478, rel=0, abs=1e-12)


def test_high_poisson_mode_exceeds_iteration_budget_explicitly():
    values = (*BASE[:4], 800.0, 0.0, 0.02)
    _, _, _, _, option = market(values, iterations=100)
    with pytest.raises(ItofinError):
        option.npv()


@pytest.mark.parametrize("volatility", [-0.1, math.nan, math.inf, -math.inf])
def test_quote_backed_volatility_factory_rejects_invalid_initial_quote(volatility):
    with pytest.raises(ItofinError):
        BlackConstantVol.from_quote(REFERENCE, SimpleQuote(volatility), DayCounter.actual360())


def test_unrepresentable_adjusted_poisson_mean_is_not_zero_jump():
    settings, quotes, _, engine, _ = market((100.0, 0.0, 0.0, 0.0, 1.0, -744.0, 0.0))
    option = VanillaOption(OptionType.Put, 100.0, REFERENCE + 1, settings)
    option.set_jump_diffusion_engine(engine)
    with pytest.raises(ItofinError):
        option.npv()
    quotes[5].set_value(0.0)
    assert option.npv() == 0.0
    with pytest.raises(ItofinError):
        market((100.0, 0.0, 0.0, 0.0, 1e-300, -100.0, 0.0))


def test_all_quantlib_corrected_haug_cached_prices():
    path = Path(__file__).resolve().parents[3] / "sdk/go/testdata/merton76-haug.csv"
    with path.open() as source:
        cases = list(csv.DictReader(source))
    assert len(cases) == 135
    for inputs in cases:
        values = tuple(
            float(inputs[key])
            for key in (
                "spot",
                "risk_free_rate",
                "dividend_yield",
                "volatility",
                "jump_intensity",
                "log_mean_jump",
                "log_jump_volatility",
            )
        )
        settings, _, _, engine, _ = market(values)
        option = VanillaOption(
            OptionType.Call, float(inputs["strike"]), REFERENCE + int(inputs["maturity_days"]), settings
        )
        option.set_jump_diffusion_engine(engine)
        actual = option.npv()
        assert actual == pytest.approx(float(inputs["expected"]), rel=0, abs=float(inputs["absolute_tolerance"]))
        assert actual == pytest.approx(float(inputs["quantlib_value"]), rel=0, abs=1e-8)
