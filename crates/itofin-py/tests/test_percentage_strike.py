"""Percentage-strike ratios expose a linear payoff through the existing FD engine."""

import gc
import math

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.pricingengines import FdBlackScholesVanillaEngine
from itofin.processes import BlackScholesProcess
from itofin.quotes import SimpleQuote
from itofin.termstructures import BlackConstantVol, BlackVarianceSurface, FlatForward
from itofin.time import Date, DayCounter

TODAY = Date(15, 6, 2026)


def market():
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    process = BlackScholesProcess(100, 0.05, 0.02, 0.2, TODAY, DayCounter.actual360())
    return settings, process


@pytest.mark.parametrize("m", [math.nan, math.inf, -math.inf])
def test_percentage_strike_rejects_nonfinite_ratios(m):
    with pytest.raises(ValueError, match="moneyness must be finite"):
        VanillaOption.percentage_strike(OptionType.Call, m, TODAY + 360, Settings())


@pytest.mark.parametrize(
    "kind,m",
    [
        (OptionType.Call, -0.2),
        (OptionType.Call, 0),
        (OptionType.Call, 0.8),
        (OptionType.Call, 1),
        (OptionType.Put, 0.8),
        (OptionType.Put, 1.2),
    ],
)
def test_percentage_strike_fd_and_linear_law(kind, m):
    settings, process = market()
    option = VanillaOption.percentage_strike(kind, m, TODAY + 360, settings)
    with pytest.raises(ItofinError):
        option.npv()
    with pytest.raises(ItofinError):
        option.price(process)
    engine = FdBlackScholesVanillaEngine(process, t_grid=400, x_grid=400)
    option.set_fd_engine(engine)
    factor = max(1 - m, 0) if kind == OptionType.Call else max(m - 1, 0)
    delta = factor * math.exp(-0.02)
    for got, expected, tolerance in [
        (option.npv(), 100 * delta, 0.0002),
        (option.delta(), delta, 0.000003),
        (option.gamma(), 0, 1e-7),
        (option.theta(), 2 * delta, 0.0002),
    ]:
        assert math.isfinite(got)
        assert got == pytest.approx(expected, rel=0, abs=tolerance)
    for query in [option.vega, option.rho, option.dividend_rho, option.error_estimate]:
        with pytest.raises(ItofinError):
            query()


def test_percentage_strike_live_quotes_expiry_snapshot_and_lifetime():
    settings, _ = market()
    dc = DayCounter.actual360()
    rate, dividend, volatility = SimpleQuote(0.05), SimpleQuote(0.02), SimpleQuote(0.2)
    risk, div = FlatForward.from_quote(TODAY, rate, dc), FlatForward.from_quote(TODAY, dividend, dc)
    vol = BlackConstantVol.from_quote(TODAY, volatility, dc)
    process = BlackScholesProcess.from_curves(100, risk, div, vol)
    engine = FdBlackScholesVanillaEngine(process, t_grid=400, x_grid=400)
    option = VanillaOption.percentage_strike(OptionType.Call, 0.8, TODAY + 360, settings)
    before = option.price_fd(engine)
    snapshot = option.results()
    del process, engine, risk, div, vol, dc
    gc.collect()
    assert option.npv() == before
    for quote, value in [(rate, 0.07), (dividend, 0.04), (volatility, 0.3)]:
        quote.set_value(value)
        assert not option.is_calculated()
        assert math.isfinite(option.npv())
    fresh_process = BlackScholesProcess(100, 0.07, 0.04, 0.3, TODAY, DayCounter.actual360())
    fresh_engine = FdBlackScholesVanillaEngine(fresh_process, t_grid=400, x_grid=400)
    fresh = VanillaOption.percentage_strike(OptionType.Call, 0.8, TODAY + 360, settings)
    value = option.npv()
    assert value == pytest.approx(fresh.price_fd(fresh_engine), rel=0, abs=1e-12)
    assert abs(value - before) > 0.1
    assert snapshot.npv == before
    settings.set_evaluation_date(TODAY + 360)
    assert option.npv() == 0
    settings.set_evaluation_date(TODAY)
    assert option.npv() == value
    del settings
    gc.collect()
    assert option.npv() == value


def test_percentage_strike_does_not_rescale_smile_strike():
    settings, _ = market()
    dc = DayCounter.actual360()
    risk, div = FlatForward(TODAY, 0.05, dc), FlatForward(TODAY, 0.02, dc)
    vol = BlackVarianceSurface(TODAY, [TODAY + 360, TODAY + 720], [80, 120], [[0.2, 0.2], [0.2, 0.2]], dc)
    process = BlackScholesProcess.from_curves(100, risk, div, vol)
    engine = FdBlackScholesVanillaEngine(process)
    option = VanillaOption.percentage_strike(OptionType.Call, 0.8, TODAY + 360, settings)
    with pytest.raises(ItofinError):
        option.price_fd(engine)
