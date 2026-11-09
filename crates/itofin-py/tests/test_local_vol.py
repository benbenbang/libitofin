"""Local volatility through the Python facade (#1366).

Mirrors the core #1365 MC European local-vol oracle on the same market, grid
and seed. The smile case uses a convex skew: the binding surface defaults to
bilinear, under which #1365's bicubic tanh grid has a negative local vol^2.
#1399 adds interpolation="bicubic", under which the tanh grid reproduces the
core oracle. CORE_TANH_VOLS pins the core fixture vols 0.20 - 0.1 tanh(2m) at
m = 0.2k as Apple libm computes them: glibc tanh is one ulp away at k = -1 and
k = 3, so the inputs are fed as literals, and CORE_STRIKES pins 100 exp(0.2k)
the same way. The NPV/se literals were captured on
macOS arm64 and are checked to abs 1e-8: Linux glibc differs by about 1e-9
through libm exp/log in the MC path (Call K115 1.0751944774791173 vs
1.0751944785024798). Bit equality with the core is asserted in-process by the
C ABI test on every platform.
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
CORE_STRIKES = [
    24.659696394160644,
    30.1194211912202,
    36.787944117144235,
    44.932896411722155,
    54.88116360940264,
    67.03200460356393,
    81.87307530779819,
    100.0,
    122.14027581601698,
    149.18246976412703,
    182.21188003905093,
    222.55409284924679,
    271.8281828459045,
    332.01169227365483,
    405.5199966844675,
]
CORE_TANH_VOLS = [
    0.29926315202011283,
    0.29836748576936806,
    0.2964027580075817,
    0.29216685544064713,
    0.28336546070121554,
    0.2664036770267849,
    0.2379948962255225,
    0.2,
    0.16200510377447752,
    0.1335963229732151,
    0.11663453929878446,
    0.10783314455935288,
    0.10359724199241832,
    0.10163251423063198,
    0.1007368479798872,
]
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

    def tanh_smile(self, interpolation, ks=range(-7, 8)):
        strikes = [CORE_STRIKES[k + 7] for k in ks]
        vols = [[CORE_TANH_VOLS[k + 7]] * 3 for k in ks]
        dates = [TODAY + 60, TODAY + 120, TODAY + 240]
        return BlackVarianceSurface(TODAY, dates, strikes, vols, DC, interpolation=interpolation)

    def mc(self, process, option_type, strike, days, samples=20000):
        option = VanillaOption(option_type, strike, TODAY + days, self.settings)
        npv = option.price_mc(MCEuropeanEngine(process, steps_per_year=12, samples=samples, seed=42))
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


def test_tanh_smile_under_bicubic_matches_the_core_oracle():
    m = Market()
    _, process = m.process(m.tanh_smile("bicubic"))
    flat = BlackScholesProcess(100.0, 0.05, 0.02, 0.20, TODAY, DC)
    for option_type, strike, sign, npv, error in (
        (OptionType.Call, 115.0, -1, 1.0751944785024798, 0.05164019195806817),
        (OptionType.Put, 85.0, 1, 1.0122245200291142, 0.05721888187042385),
    ):
        base = m.analytic(flat, option_type, strike, 180)
        _, mc, se = m.mc(process, option_type, strike, 180, samples=4000)
        assert abs(mc - npv) < 1e-8 and abs(se - error) < 1e-8, (strike, mc, se)
        assert sign * (mc - base) > 5 * se, (strike, mc, base, se)


def test_interpolation_choice():
    m = Market()
    strikes, dates = [90.0, 100.0, 110.0], [TODAY + 60, TODAY + 120, TODAY + 240]
    vols = [[0.22, 0.21, 0.2], [0.2, 0.2, 0.2], [0.18, 0.19, 0.2]]
    default = BlackVarianceSurface(TODAY, dates, strikes, vols, DC)
    bilinear = BlackVarianceSurface(TODAY, dates, strikes, vols, DC, interpolation="bilinear")
    assert default.black_vol(0.25, 95.0) == bilinear.black_vol(0.25, 95.0)
    lin, cub = m.tanh_smile("bilinear"), m.tanh_smile("bicubic")
    assert abs(lin.black_vol(0.25, 95.0) - cub.black_vol(0.25, 95.0)) > 1e-6
    with pytest.raises(ValueError, match="unknown interpolation"):
        m.tanh_smile("cubic")
    with pytest.raises(ItofinError, match="bilinear interpolation needs at least 2"):
        m.tanh_smile("bicubic", ks=range(1))
