"""QuantLib 1.43 COS and exponential-fitting oracles at upstream tolerances."""
import gc
import json
import math
from pathlib import Path

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.models import HestonModel
from itofin.pricingengines import CosHestonEngine
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
