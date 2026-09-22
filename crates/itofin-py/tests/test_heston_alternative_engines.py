"""QuantLib 1.43 COS and exponential-fitting oracles at upstream tolerances."""
from datetime import date, timedelta
import gc
import json
import math
from pathlib import Path
from typing import Any

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.models import HestonModel
from itofin.pricingengines import CosHestonEngine, ExponentialFittingControlVariate, ExponentialFittingHestonEngine
from itofin.processes import HestonProcess
from itofin.time import Date, DayCounter


def market(reference: Date, rates: tuple[float, float, float], params: tuple[float, float, float, float, float]):
    settings = Settings()
    settings.set_evaluation_date(reference)
    process = HestonProcess(*rates, *params, reference, DayCounter.actual365_fixed())
    return settings, HestonModel(process)


def test_cos_heston_cached_and_retained():
    settings, model = market(Date(7, 2, 2017), (.15, .07, 100), (.1, 4, .22, 1.8, -.75))
    engine = CosHestonEngine(model, 25, 600)
    for kind, strike, expected in [(OptionType.Call, 120, 9.364410588426075), (OptionType.Call, 250, .01036797658132471), (OptionType.Put, 80, 5.319092971836708), (OptionType.Put, 10, .01032681906278383)]:
        option = VanillaOption(kind, strike, Date(7, 2, 2018), settings)
        assert abs(option.price_cos_heston(engine) - expected) < 1e-10
    option.set_cos_heston_engine(engine)
    del model, engine
    gc.collect()
    assert abs(option.npv() - .01032681906278383) < 1e-10


def test_exponential_fitting_quantlib_grid():
    fixture = json.loads((Path(__file__).resolve().parents[3] / "sdk/go/testdata/heston_exponential.json").read_text())
    reference = Date(13, 5, 2020)
    settings, model = market(reference, (.0507, .0469, 1), (.04, 2.5, .06, .75, -.6))
    engine = ExponentialFittingHestonEngine(model)
    for i, days in enumerate(fixture["days"]):
        t = days / 365
        discount = math.exp(-.0507 * t)
        forward = math.exp((.0507 - .0469) * t)
        for j, m in enumerate(fixture["moneyness"]):
            strike = math.exp(-m * math.sqrt(.06 * t)) * forward
            base = fixture["prices"][i * 11 + j]
            for kind in [OptionType.Call, OptionType.Put]:
                intrinsic = (forward - strike) * discount
                expected = base + max(intrinsic if kind == OptionType.Call else -intrinsic, 0)
                expiry = date(2020, 5, 13) + timedelta(days=days)
                option = VanillaOption(kind, strike, Date(expiry.day, expiry.month, expiry.year), settings)
                assert abs(option.price_exponential_fitting_heston(engine) - expected) < 1e-8
    option.set_exponential_fitting_heston_engine(engine)
    del model, engine
    gc.collect()
    assert abs(option.npv() - expected) < 1e-8


@pytest.mark.parametrize("cv", [ExponentialFittingControlVariate.Optimal, ExponentialFittingControlVariate.AndersenPiterbarg, ExponentialFittingControlVariate.AndersenPiterbargOptCV, ExponentialFittingControlVariate.AsymptoticChF, ExponentialFittingControlVariate.AngledContour, ExponentialFittingControlVariate.AngledContourNoCV])
def test_exponential_fitting_control_variates(cv):
    fixture = json.loads((Path(__file__).resolve().parents[3] / "sdk/go/testdata/heston_control_variates.json").read_text())
    settings, model = market(Date(7, 2, 2017), (.15, .07, 100), (.1, 4, .22, 1.8, -.75))
    option = VanillaOption(OptionType.Call, 120, Date(7, 2, 2018), settings)
    engine = ExponentialFittingHestonEngine(model, cv)
    assert abs(option.price_exponential_fitting_heston(engine) - fixture["prices"][int(cv)]) < 1e-10


def test_heston_engine_invalid_configuration_and_recovery():
    fixture = json.loads((Path(__file__).resolve().parents[3] / "sdk/go/testdata/heston_control_variates.json").read_text())
    settings, model = market(Date(7, 2, 2017), (.15, .07, 100), (.1, 4, .22, 1.8, -.75))
    for l, n in [(0, 200), (math.nan, 200), (16, 0)]:
        with pytest.raises(ItofinError):
            CosHestonEngine(model, l, n)
    for kwargs in [{"scaling": 0}, {"scaling": math.nan}, {"control_variate": ExponentialFittingControlVariate.AsymptoticChF, "alpha": -.3}, {"alpha": math.nan}, {"alpha": math.inf}]:
        with pytest.raises(ItofinError):
            ExponentialFittingHestonEngine(model, **kwargs)
    option = VanillaOption(OptionType.Call, 120, Date(7, 2, 2018), settings)
    engine = ExponentialFittingHestonEngine(model, scaling=1)
    assert abs(option.price_exponential_fitting_heston(engine) - fixture["fixed_scaling_price"]) < 1e-10


def test_exponential_fitting_control_variate_integer_contract():
    values = [ExponentialFittingControlVariate.Optimal, ExponentialFittingControlVariate.AndersenPiterbarg, ExponentialFittingControlVariate.AndersenPiterbargOptCV, ExponentialFittingControlVariate.AsymptoticChF, ExponentialFittingControlVariate.AngledContour, ExponentialFittingControlVariate.AngledContourNoCV]
    assert [int(v) for v in values] == list(range(6))
    with pytest.raises(TypeError):
        dynamic_enum: Any = ExponentialFittingControlVariate
        dynamic_enum(0)


def test_cos_heston_inspectors_quantlib_and_retention():
    fixture = json.loads((Path(__file__).resolve().parents[3] / "sdk/go/testdata/cos_heston_inspectors.json").read_text())
    settings, model = market(Date(7, 2, 2017), (.15, .075, 100), (.1, 4, .25, .4, -.75))
    engine = CosHestonEngine(model)
    del model, settings
    gc.collect()
    assert len(fixture["cumulants"]) == 13
    assert len(fixture["characteristic"]) == 16
    assert len(fixture["mu"]) == 4
    inspectors = [engine.c1, engine.c2, engine.c3, engine.c4]
    for t, *expected in fixture["cumulants"]:
        for fn, value in zip(inspectors, expected):
            assert abs(fn(t) - value) < 1e-10
    for t, u, real, imaginary in fixture["characteristic"]:
        actual = engine.chf(u, t)
        assert abs(actual[0] - real) < 1e-12
        assert abs(actual[1] - imaginary) < 1e-12
    for t, value in fixture["mu"]:
        assert abs(engine.mu_t(t) - value) < 1e-12
    for fn in [*inspectors, engine.mu_t]:
        assert fn(0) == 0
        for t in [-1, math.nan, math.inf]:
            with pytest.raises(ItofinError):
                fn(t)
    for u, t in [(math.nan, 1), (math.inf, 1), (1, -1), (1, math.inf)]:
        with pytest.raises(ItofinError):
            engine.chf(u, t)
    with pytest.raises(ItofinError):
        engine.c4(1e6)
    assert abs(engine.c4(fixture["cumulants"][0][0]) - fixture["cumulants"][0][4]) < 1e-10
    assert engine.chf(0, 1) == (1, 0)
