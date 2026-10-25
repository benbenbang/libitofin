"""Independent regular-regime cash-digital formulas and facade domain contracts."""
import math
from concurrent.futures import ThreadPoolExecutor
from typing import Any, cast

import pytest

from itofin import ItofinError
from itofin.instruments import OptionType
from itofin.pricingengines import BlackCalculator


def queries(calculator, spot=97., maturity=2.):
    return {
        "value": calculator.value(), "alpha": calculator.alpha(),
        "beta": calculator.beta(), "delta_forward": calculator.delta_forward(),
        "delta": calculator.delta(spot), "gamma_forward": calculator.gamma_forward(),
        "gamma": calculator.gamma(spot),
        "elasticity_forward": calculator.elasticity_forward(),
        "elasticity": calculator.elasticity(spot),
        "theta": calculator.theta(spot, maturity),
        "theta_per_day": calculator.theta_per_day(spot, maturity),
        "vega": calculator.vega(maturity), "rho": calculator.rho(maturity),
        "dividend_rho": calculator.dividend_rho(maturity),
        "itm_cash_probability": calculator.itm_cash_probability(),
        "itm_asset_probability": calculator.itm_asset_probability(),
        "strike_sensitivity": calculator.strike_sensitivity(),
    }


@pytest.mark.parametrize("option,sign", [(OptionType.Call, 1), (OptionType.Put, -1)])
@pytest.mark.parametrize("strike", [80., 100., 120.])
@pytest.mark.parametrize("forward", [90., 110.])
@pytest.mark.parametrize("deviation", [.15, .5, 1.2])
@pytest.mark.parametrize("cash", [-7., 0., 13.])
def test_cash_digital_analytic(option, sign, strike, forward, deviation, cash):
    discount, spot, maturity = .94, 97., 2.
    d1 = math.log(forward / strike) / deviation + deviation / 2
    d2 = d1 - deviation
    cdf = lambda x: math.erfc(-x / math.sqrt(2)) / 2
    density = math.exp(-d2 * d2 / 2) / math.sqrt(2 * math.pi)
    beta = cdf(sign * d2)
    value = discount * cash * beta
    delta = discount * cash * sign * density / (deviation * forward)
    gamma = -delta / forward * (1 + d2 / deviation)
    theta = -(math.log(discount) * value + math.log(forward / spot) * forward * delta
              + .5 * deviation * deviation * forward * forward * gamma) / maturity
    if value > math.ulp(1.):
        elasticity = delta / value * forward
    elif abs(delta) < math.ulp(1.):
        elasticity = 0.
    else:
        elasticity = math.copysign(float.fromhex('0x1.fffffffffffffp+1023'), delta)
    expected = {
        "value": value, "alpha": 0., "beta": beta,
        "delta_forward": delta, "delta": delta * forward / spot,
        "gamma_forward": gamma, "gamma": gamma * (forward / spot) ** 2,
        "elasticity_forward": elasticity, "elasticity": elasticity,
        "theta": theta, "theta_per_day": theta / 365,
        "vega": discount * cash * sign * density
        * (math.log(strike / forward) / (deviation * deviation) - .5) * math.sqrt(maturity),
        "rho": maturity * (forward * delta - value),
        "dividend_rho": -maturity * forward * delta,
        "itm_cash_probability": cdf(d2), "itm_asset_probability": cdf(d1),
        "strike_sensitivity": -delta * forward / strike,
    }
    calculator = BlackCalculator.cash_digital(option, strike, cash, forward, deviation, discount)
    for name, actual in queries(calculator, spot, maturity).items():
        assert actual == pytest.approx(expected[name], abs=2e-11, rel=2e-12), name


@pytest.mark.parametrize("parameter", range(5))
@pytest.mark.parametrize("bad", [math.nan, math.inf, -math.inf])
def test_cash_digital_finite_constructor(parameter, bad):
    args = [100., 7., 110., .2, .95]
    args[parameter] = bad
    with pytest.raises(ItofinError):
        BlackCalculator.cash_digital(OptionType.Call, *args)


@pytest.mark.parametrize("deviation", [-1., 0., 1e-20, math.ulp(1.)])
def test_cash_digital_excludes_degenerate_core_shortcut(deviation):
    with pytest.raises(ItofinError, match="must exceed f64::EPSILON"):
        BlackCalculator.cash_digital(OptionType.Call, 100, 7, 110, deviation, .95)
    calculator = BlackCalculator.cash_digital(
        OptionType.Call, 100, 7, 110, math.nextafter(math.ulp(1.), math.inf), .95)
    assert calculator.value() == pytest.approx(6.65)
    assert BlackCalculator(OptionType.Call, 100, 110, 0, .95).value() == pytest.approx(9.5)


def test_cash_digital_types_domains_and_edges():
    with pytest.raises(TypeError):
        BlackCalculator.cash_digital(cast(Any, 0), 100, 7, 110, .2, .95)
    for args in [(-1, 7, 110, .2, .95), (100, 7, 0, .2, .95),
                 (100, 7, -1, .2, .95), (100, 7, 110, .2, 0)]:
        with pytest.raises(ItofinError):
            BlackCalculator.cash_digital(OptionType.Call, *args)
    calculator = BlackCalculator.cash_digital(OptionType.Call, 0, 7, 110, .2, .95)
    assert calculator.value() == pytest.approx(6.65)
    for query in [calculator.gamma_forward, calculator.strike_sensitivity,
                  lambda: calculator.gamma(100), lambda: calculator.vega(2),
                  lambda: calculator.theta(100, 2), lambda: calculator.theta_per_day(100, 2)]:
        with pytest.raises(ItofinError, match="result must be finite"):
            query()
    assert calculator.theta(math.nan, 0) == 0
    overflow = BlackCalculator.cash_digital(OptionType.Call, 100, 1e308, 110, .2, 10)
    with pytest.raises(ItofinError, match="result must be finite"):
        overflow.value()
    calculator = BlackCalculator.cash_digital(OptionType.Put, 100, 7, 110, .2, .95)
    with pytest.raises((AttributeError, TypeError)):
        setattr(calculator, "cash_payoff", 3)
    for name in ("delta", "gamma", "elasticity"):
        for bad in (0., -1., math.nan, math.inf):
            with pytest.raises(ItofinError):
                getattr(calculator, name)(bad)
    for name in ("vega", "rho", "dividend_rho"):
        for bad in (-1., math.nan, math.inf):
            with pytest.raises(ItofinError):
                getattr(calculator, name)(bad)
    with ThreadPoolExecutor(max_workers=4) as pool:
        assert list(pool.map(lambda _: calculator.value(), range(16))) == [calculator.value()] * 16


def native_cases():
    import csv
    from pathlib import Path
    fixture = Path(__file__).resolve().parents[3] / "sdk/go/testdata/cash_digital_black_calculator.tsv"
    return list(csv.DictReader((line for line in fixture.read_text().splitlines()
                               if not line.startswith("#")), delimiter="\t"))


@pytest.mark.parametrize("row", native_cases(), ids=lambda row: row["id"])
def test_cash_digital_native_queries(row):
    option = OptionType.Call if row["option"] == "0" else OptionType.Put
    args = [float(row[name]) for name in ("strike", "cash_payoff", "forward", "std_dev", "discount")]
    if row["domain"] == "diagnostic_rejected":
        with pytest.raises(ItofinError, match="must exceed f64::EPSILON"):
            BlackCalculator.cash_digital(option, *args)
        return
    calculator = BlackCalculator.cash_digital(option, *args)
    spot, maturity = float(row["spot"]), float(row["maturity"])
    for name in ("value", "alpha", "beta", "delta_forward", "delta", "elasticity_forward",
                 "elasticity", "gamma_forward", "gamma", "theta", "theta_per_day", "vega",
                 "rho", "dividend_rho", "strike_sensitivity", "itm_cash_probability", "itm_asset_probability"):
        arguments = ((spot, maturity) if name in ("theta", "theta_per_day")
                     else (spot,) if name in ("delta", "gamma", "elasticity")
                     else (maturity,) if name in ("vega", "rho", "dividend_rho") else ())
        expected_value = row[name]
        if expected_value in ("nan", "+inf", "-inf", "error"):
            with pytest.raises(ItofinError):
                getattr(calculator, name)(*arguments)
        else:
            assert getattr(calculator, name)(*arguments) == pytest.approx(
                float(expected_value), abs=2e-11, rel=2e-12), name
