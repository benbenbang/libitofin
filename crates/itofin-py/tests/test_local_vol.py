"""Local volatility through the Python facade (#1366).

Mirrors the core #1365 MC European local-vol oracle on the same market, grid
and seed. The smile case uses a convex skew: the binding surface is bilinear,
under which #1365's bicubic tanh grid has a negative local vol^2.
"""

import math

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.pricingengines import MCEuropeanEngine
from itofin.processes import BlackScholesProcess
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    BlackVarianceCurve,
    BlackVarianceSurface,
    FlatForward,
    LocalVolCurve,
    LocalVolSurface,
)
from itofin.time import Date, DayCounter

TODAY = Date(15, 6, 2026)
DC = DayCounter.actual360()


class Market:
    def __init__(self):
        self.settings = Settings()
        self.settings.set_evaluation_date(TODAY)
        self.spot = SimpleQuote(100.0)
        self.risk_free = FlatForward(TODAY, 0.05, DC)
        self.dividend = FlatForward(TODAY, 0.02, DC)

    def curve(self, vols, days, monotone=True):
        return BlackVarianceCurve(TODAY, [TODAY + d for d in days], vols, DC, monotone)

    def smile(self):
        strikes = [100.0 * math.exp(0.2 * k) for k in range(-7, 8)]
        vols = [[0.2 - 0.1 * math.log(k / 100) + 0.05 * math.log(k / 100) ** 2] * 3 for k in strikes]
        return BlackVarianceSurface(TODAY, [TODAY + 60, TODAY + 120, TODAY + 240], strikes, vols, DC)

    def process(self, black, local=None):
        local = local or LocalVolSurface(black, self.risk_free, self.dividend, self.spot)
        process = BlackScholesProcess.with_local_vol(
            self.spot, self.risk_free, self.dividend, black, local
        )
        return local, process

    def mc(self, process, option_type, strike, days):
        option = VanillaOption(option_type, strike, TODAY + days, self.settings)
        npv = option.price_mc(MCEuropeanEngine(process, steps_per_year=12, samples=20000, seed=42))
        return option, npv, option.error_estimate()

    def analytic(self, process, option_type, strike, days):
        return VanillaOption(option_type, strike, TODAY + days, self.settings).price(process)


def black(option_type, strike, variance):
    forward, sd = 100.0 * math.exp(0.03), math.sqrt(variance)
    d1 = math.log(forward / strike) / sd + sd / 2

    def n(x):
        return 0.5 * math.erfc(-x / math.sqrt(2))

    if option_type == OptionType.Call:
        return math.exp(-0.05) * (forward * n(d1) - strike * n(d1 - sd))
    return math.exp(-0.05) * (strike * n(sd - d1) - forward * n(-d1))


def euler_variance(local):
    return sum(local.local_vol(i / 12, 100.0, True) ** 2 / 12 for i in range(12))


def test_time_only_local_vol_matches_analytic_european():
    m = Market()
    local, process = m.process(m.curve([0.15, 0.20, 0.25], [119, 269, 360]))
    v_euler = euler_variance(local)
    for option_type in (OptionType.Call, OptionType.Put):
        for strike in (90.0, 100.0, 110.0):
            analytic = m.analytic(process, option_type, strike, 360)
            _, mc, se = m.mc(process, option_type, strike, 360)
            euler = black(option_type, strike, v_euler)
            budget = abs(euler - analytic)
            assert se > 0 and abs(mc - euler) < 3 * se, (strike, mc, euler, se)
            assert budget < 0.03 and abs(mc - analytic) < 3 * se + budget


def test_smile_moves_the_local_vol_price():
    m = Market()
    _, process = m.process(m.smile())
    flat = BlackScholesProcess(100.0, 0.05, 0.02, 0.20, TODAY, DC)
    for option_type, strike, sign in ((OptionType.Call, 115.0, -1), (OptionType.Put, 85.0, 1)):
        base = m.analytic(flat, option_type, strike, 180)
        _, mc, se = m.mc(process, option_type, strike, 180)
        implied = m.analytic(process, option_type, strike, 180)
        assert sign * (mc - base) > 5 * se, (strike, mc, base, se)
        assert abs(mc - implied) < 3 * se + 0.01, (strike, mc, implied, se)


def test_shared_quote_reprices():
    m = Market()
    _, process = m.process(m.curve([0.15, 0.20, 0.25], [119, 269, 360]))
    option, before, _ = m.mc(process, OptionType.Call, 100.0, 360)
    m.spot.set_value(105.0)
    assert not option.is_calculated()
    assert option.npv() > before


def test_linear_local_vol_curve():
    m = Market()
    black_curve = m.curve([0.15, 0.20, 0.25], [119, 269, 360])
    curve = LocalVolCurve(black_curve)
    assert curve.local_vol(0.1, 100.0) == pytest.approx(0.15, abs=1e-12)
    _, process = m.process(black_curve, curve)
    analytic = m.analytic(process, OptionType.Call, 100.0, 360)
    _, mc, se = m.mc(process, OptionType.Call, 100.0, 360)
    v_euler = euler_variance(curve)
    budget = abs(black(OptionType.Call, 100.0, v_euler) - analytic)
    assert abs(mc - analytic) < 3 * se + budget


def test_core_errors():
    m = Market()
    local, _ = m.process(m.curve([0.15, 0.20, 0.25], [119, 269, 360]))
    with pytest.raises(ItofinError, match="past max curve time"):
        local.local_vol(1.5, 100.0)
    with pytest.raises(ItofinError, match="past max curve time"):
        LocalVolCurve(m.curve([0.15, 0.20], [119, 269])).local_vol(1.5, 100.0)
    _, process = m.process(m.curve([0.30, 0.10], [102, 252], monotone=False))
    with pytest.raises(ItofinError, match="decreasing variance"):
        m.mc(process, OptionType.Call, 100.0, 180)
    plain = BlackScholesProcess.from_quote(m.spot, m.risk_free, m.dividend, m.smile())
    with pytest.raises(ItofinError, match="only constant Black volatilities"):
        m.mc(plain, OptionType.Call, 100.0, 180)
