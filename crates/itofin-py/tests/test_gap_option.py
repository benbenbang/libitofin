"""The gap factory is FD-backed, signed, and distinct from vanilla analytic pricing."""

import gc
import math

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.pricingengines import FdBlackScholesVanillaEngine
from itofin.processes import BlackScholesProcess
from itofin.quotes import SimpleQuote
from itofin.termstructures import BlackConstantVol, FlatForward
from itofin.time import Date, DayCounter

TODAY = Date(15, 6, 2026)


def market():
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    process = BlackScholesProcess(100.0, 0.05, 0.02, 0.2, TODAY, DayCounter.actual360())
    return settings, process


@pytest.mark.parametrize("kind", [OptionType.Call, OptionType.Put])
def test_gap_factory_preserves_plain_limit_and_rejects_analytic(kind):
    settings, process = market()
    option = VanillaOption.gap(kind, 100.0, 100.0, TODAY + 360, settings)
    with pytest.raises(ItofinError):
        option.npv()
    option.set_engine(process)
    with pytest.raises(ItofinError):
        option.npv()
    engine = FdBlackScholesVanillaEngine(process, t_grid=400, x_grid=400, damping_steps=2)
    vanilla = VanillaOption(kind, 100.0, TODAY + 360, settings)
    expected = vanilla.price_fd(engine)
    assert math.isfinite(expected)
    assert option.price_fd(engine) == pytest.approx(expected, rel=0, abs=1e-12)
    assert all(math.isfinite(query()) for query in [option.npv, option.delta, option.gamma, option.theta])
    with pytest.raises(ItofinError):
        option.vega()


@pytest.mark.parametrize("kind,second", [(OptionType.Call, 160.0), (OptionType.Put, 40.0)])
def test_gap_negative_values_and_retained_graph(kind, second):
    settings, process = market()
    option = VanillaOption.gap(kind, 100.0, second, TODAY + 360, settings)
    engine = FdBlackScholesVanillaEngine(process, t_grid=400, x_grid=400, damping_steps=2)
    initial = option.price_fd(engine)
    assert math.isfinite(initial) and initial < 0
    snapshot = option.results()
    del process, engine
    gc.collect()
    assert option.npv() == initial
    settings.set_evaluation_date(TODAY + 360)
    assert option.npv() == 0.0
    settings.set_evaluation_date(TODAY)
    assert option.npv() == initial
    assert snapshot.npv == initial
    del settings
    gc.collect()
    assert option.npv() == initial


def test_gap_live_curves_and_volatility_invalidate_pricing():
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    dc = DayCounter.actual360()
    rate, dividend, volatility = SimpleQuote(0.05), SimpleQuote(0.02), SimpleQuote(0.2)
    risk = FlatForward.from_quote(TODAY, rate, dc)
    div = FlatForward.from_quote(TODAY, dividend, dc)
    vol = BlackConstantVol.from_quote(TODAY, volatility, dc)
    process = BlackScholesProcess.from_curves(100.0, risk, div, vol)
    engine = FdBlackScholesVanillaEngine(process, t_grid=400, x_grid=400, damping_steps=2)
    option = VanillaOption.gap(OptionType.Call, 105.0, 120.0, TODAY + 360, settings)
    before = option.price_fd(engine)
    assert math.isfinite(before)
    del process, engine, risk, div, vol, dc
    gc.collect()
    for quote, updated in [(rate, 0.07), (dividend, 0.04), (volatility, 0.3)]:
        quote.set_value(updated)
        assert not option.is_calculated()
        after = option.npv()
        assert math.isfinite(after) and abs(after - before) > 1e-8
        before = after
    fresh_process = BlackScholesProcess(100.0, 0.07, 0.04, 0.3, TODAY, DayCounter.actual360())
    fresh_engine = FdBlackScholesVanillaEngine(fresh_process, t_grid=400, x_grid=400, damping_steps=2)
    fresh = VanillaOption.gap(OptionType.Call, 105.0, 120.0, TODAY + 360, settings)
    assert option.npv() == pytest.approx(fresh.price_fd(fresh_engine), rel=0, abs=1e-12)


@pytest.mark.parametrize(
    "strike,second",
    [(-1.0, 100.0), (math.nan, 100.0), (math.inf, 100.0), (100.0, math.nan), (100.0, math.inf), (100.0, -math.inf)],
)
def test_gap_factory_rejects_invalid_numeric_parameters(strike, second):
    with pytest.raises(ValueError):
        VanillaOption.gap(OptionType.Call, strike, second, TODAY + 360, Settings())


@pytest.mark.parametrize("strike,second", [(0.0, -1.0), (100.0, 0.0), (100.0, -20.0)])
def test_gap_factory_accepts_finite_signed_second_strike(strike, second):
    option = VanillaOption.gap(OptionType.Call, strike, second, TODAY + 360, Settings())
    with pytest.raises(ItofinError):
        option.npv()
