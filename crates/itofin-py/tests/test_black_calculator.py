"""Independent analytical plain-vanilla formulas and native edge contracts."""
import math
from concurrent.futures import ThreadPoolExecutor
from typing import Any, cast

import pytest

from itofin import ItofinError
from itofin.instruments import OptionType
from itofin.pricingengines import BlackCalculator


def expected(sign, strike, forward, deviation, discount, spot, maturity):
    """Derive European lognormal payoffs and Greeks analytically, not a QL fixture."""
    d1 = math.log(forward / strike) / deviation + deviation / 2
    d2 = d1 - deviation
    cdf = lambda x: math.erfc(-x / math.sqrt(2)) / 2
    density = math.exp(-d1 * d1 / 2) / math.sqrt(2 * math.pi)
    alpha, beta = sign * cdf(sign * d1), -sign * cdf(sign * d2)
    value = discount * (forward * alpha + strike * beta)
    delta_forward = discount * alpha
    gamma_forward = discount * density / (forward * deviation)
    r = -math.log(discount) / maturity
    q = r - math.log(forward / spot) / maturity
    theta = (-discount * forward * density * deviation / (2 * maturity)
             + q * discount * forward * alpha + r * discount * strike * beta)
    return {
        "value": value, "alpha": alpha, "beta": beta,
        "delta_forward": delta_forward, "delta": delta_forward * forward / spot,
        "gamma_forward": gamma_forward, "gamma": gamma_forward * (forward / spot) ** 2,
        "elasticity_forward": delta_forward * forward / value,
        "elasticity": delta_forward * forward / value,
        "theta": theta, "theta_per_day": theta / 365,
        "vega": discount * forward * density * math.sqrt(maturity),
        "rho": -maturity * discount * strike * beta,
        "dividend_rho": -maturity * discount * forward * alpha,
        "itm_cash_probability": cdf(d2), "itm_asset_probability": cdf(d1),
        "strike_sensitivity": discount * beta,
    }


def query(c, name, spot=97., maturity=2.):
    if name in ("theta", "theta_per_day"):
        return getattr(c, name)(spot, maturity)
    if name in ("delta", "gamma", "elasticity"):
        return getattr(c, name)(spot)
    if name in ("vega", "rho", "dividend_rho"):
        return getattr(c, name)(maturity)
    return getattr(c, name)()


@pytest.mark.parametrize("option,sign", [(OptionType.Call, 1), (OptionType.Put, -1)])
@pytest.mark.parametrize("strike", [80., 100., 120.])
@pytest.mark.parametrize("forward", [90., 110.])
@pytest.mark.parametrize("deviation", [.15, .5, 1.2])
def test_all_scalar_queries_analytical(option, sign, strike, forward, deviation):
    c = BlackCalculator(option, strike, forward, deviation, .94)
    for name, want in expected(sign, strike, forward, deviation, .94, 97., 2.).items():
        assert query(c, name) == pytest.approx(want, abs=2e-11, rel=2e-12), name


@pytest.mark.parametrize("option,sign", [(OptionType.Call, 1), (OptionType.Put, -1)])
@pytest.mark.parametrize("forward", [90., 100., 110.])
@pytest.mark.parametrize("deviation", [0., 1e-20])
def test_zero_vol_native_ladder(option, sign, forward, deviation):
    c = BlackCalculator(option, 100., forward, deviation, .95)
    units = .5 if forward == 100 else float((forward > 100) == (sign > 0))
    assert c.delta_forward() == sign * units * .95
    assert c.strike_sensitivity() == -sign * units * .95
    assert c.gamma(97) == c.gamma_forward() == c.vega(2) == 0
    assert c.value() == pytest.approx(.95 * max(sign * (forward - 100), 0))
    assert c.theta(math.nan, 0) == c.theta_per_day(-1, 0) == 0


def test_zero_strike_nonfinite_policy_and_extreme_results():
    c = BlackCalculator(OptionType.Call, 0, 100, .2, .95)
    assert c.value() == 95
    assert c.itm_cash_probability() == c.itm_asset_probability() == 1
    for name in ("gamma", "gamma_forward", "vega", "strike_sensitivity", "theta", "theta_per_day"):
        with pytest.raises(ItofinError, match="result must be finite"):
            query(c, name)
    huge = BlackCalculator(OptionType.Call, 0, 1e308, 0, 10)
    with pytest.raises(ItofinError, match="result must be finite"):
        huge.value()
    for option in (OptionType.Call, OptionType.Put):
        c = BlackCalculator(option, 100, 100, 0, 1)
        assert c.elasticity_forward() == (float.fromhex('0x1.fffffffffffffp+1023') if option == OptionType.Call
                                          else -float.fromhex('0x1.fffffffffffffp+1023'))


@pytest.mark.parametrize("parameter", range(4))
@pytest.mark.parametrize("bad", [math.nan, math.inf, -math.inf, -1.])
def test_constructor_validation(parameter, bad):
    args = [100., 100., .2, .95]
    args[parameter] = bad
    with pytest.raises(ItofinError):
        BlackCalculator(OptionType.Call, *args)


def test_types_domains_export_immutable_and_shared_reads():
    assert BlackCalculator.__module__ == "itofin.pricingengines"
    with pytest.raises(TypeError):
        BlackCalculator(cast(Any, 0), 100, 100, .2, .95)
    for args in [(100, 0, .2, .95), (100, 110, .2, 0)]:
        with pytest.raises(ItofinError):
            BlackCalculator(OptionType.Call, *args)
    c = BlackCalculator(OptionType.Put, 100, 110, .2, .95)
    for name in ("delta", "gamma", "elasticity"):
        for bad in (0., -1., math.nan, math.inf):
            with pytest.raises(ItofinError):
                getattr(c, name)(bad)
    for name in ("vega", "rho", "dividend_rho"):
        for bad in (-1., math.nan, math.inf):
            with pytest.raises(ItofinError):
                getattr(c, name)(bad)
    for bad in (-1., math.nan, math.inf):
        with pytest.raises(ItofinError):
            c.theta(100, bad)
    with pytest.raises((AttributeError, TypeError)):
        setattr(c, "strike", 5)
    with ThreadPoolExecutor(max_workers=4) as pool:
        assert list(pool.map(lambda _: c.value(), range(16))) == [c.value()] * 16
