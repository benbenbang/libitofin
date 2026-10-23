"""Lifecycle and explicitly qualified original-core digital boundary behavior."""

import gc
import math

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.models import HestonModel
from itofin.processes import BlackScholesProcess, HestonProcess
from itofin.quotes import SimpleQuote
from itofin.termstructures import BlackConstantVol, FlatForward
from itofin.time import Date, DayCounter

TODAY = Date(15, 6, 2026)


@pytest.mark.parametrize("kind,sign", [(OptionType.Call, 1), (OptionType.Put, -1)])
@pytest.mark.parametrize("cash", [-12.0, 0.0, 12.0])
@pytest.mark.parametrize("volatility", [0.0, 1e-18])
@pytest.mark.parametrize("spot", [90.0, 100.0, 110.0])
def test_cash_digital_original_core_zero_volatility_ladder(kind, sign, cash, volatility, spot):
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    process = BlackScholesProcess(spot, 0.0, 0.0, volatility, TODAY, DayCounter.actual360())
    option = VanillaOption.cash_or_nothing(kind, 100.0, cash, TODAY + 360, settings)
    option.set_engine(process)
    probability = 0.5 if spot == 100 else float(sign * (spot - 100) > 0)
    delta = sign * probability
    assert option.npv() == cash * probability
    assert option.delta() == delta
    assert option.gamma() == 0.0
    assert option.vega() == 0.0
    assert option.rho() == spot * delta - cash * probability
    assert option.dividend_rho() == -spot * delta
    assert option.theta() == 0.0


def test_cash_digital_live_curve_and_volatility_retention_and_expiry():
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    dc = DayCounter.actual360()
    rate, dividend, volatility = SimpleQuote(0.05), SimpleQuote(0.02), SimpleQuote(0.2)
    risk_curve = FlatForward.from_quote(TODAY, rate, dc)
    dividend_curve = FlatForward.from_quote(TODAY, dividend, dc)
    vol_curve = BlackConstantVol.from_quote(TODAY, volatility, dc)
    process = BlackScholesProcess.from_curves(100.0, risk_curve, dividend_curve, vol_curve)
    option = VanillaOption.cash_or_nothing(OptionType.Call, 105.0, 12.0, TODAY + 360, settings)
    option.set_engine(process)
    before = option.npv()
    snapshot = option.results()
    del process, risk_curve, dividend_curve, vol_curve, dc
    gc.collect()
    values = [0.05, 0.02, 0.2]
    expected = before
    for index, (quote, value) in enumerate([(rate, 0.07), (dividend, 0.04), (volatility, 0.3)]):
        quote.set_value(value)
        values[index] = value
        assert not option.is_calculated()
        d2 = (math.log(100 / 105) + values[0] - values[1] - values[2] ** 2 / 2) / values[2]
        expected = 12 * math.exp(-values[0]) * math.erfc(-d2 / math.sqrt(2)) / 2
        assert option.npv() == pytest.approx(expected, rel=0, abs=2e-12)
        assert option.results().npv == option.npv()
    assert snapshot.npv == before
    settings.set_evaluation_date(TODAY + 360)
    assert option.npv() == 0.0
    assert option.delta() == 0.0
    settings.set_evaluation_date(TODAY)
    assert option.npv() == pytest.approx(expected, rel=0, abs=2e-12)
    del settings, rate, dividend, volatility
    gc.collect()
    assert option.price(BlackScholesProcess(100.0, 0.07, 0.04, 0.3, TODAY, DayCounter.actual360())) == pytest.approx(
        expected, rel=0, abs=2e-12
    )


def test_cash_digital_rejects_plain_payoff_only_heston_and_recovers():
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    dc = DayCounter.actual360()
    process = HestonProcess(0.05, 0.02, 100.0, 0.04, 1.0, 0.04, 0.3, -0.5, TODAY, dc)
    model = HestonModel(process)
    option = VanillaOption.cash_or_nothing(OptionType.Put, 100.0, 12.0, TODAY + 360, settings)
    option.set_heston_engine(model, 64)
    with pytest.raises(ItofinError, match="plain"):
        option.npv()
    option.set_engine(BlackScholesProcess(100.0, 0.05, 0.02, 0.2, TODAY, dc))
    assert option.npv() > 0


def test_cash_digital_signed_cash_scaling_and_unchanged_vanilla_factory():
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    process = BlackScholesProcess(100.0, 0.05, 0.02, 0.2, TODAY, DayCounter.actual360())
    unit = VanillaOption.cash_or_nothing(OptionType.Call, 100.0, 1.0, TODAY + 360, settings)
    unit.set_engine(process)
    for cash in [-12.0, 0.0, 12.0]:
        option = VanillaOption.cash_or_nothing(OptionType.Call, 100.0, cash, TODAY + 360, settings)
        option.set_engine(process)
        for method in ["npv", "delta", "gamma", "theta", "vega", "rho", "dividend_rho"]:
            assert getattr(option, method)() == pytest.approx(cash * getattr(unit, method)(), rel=0, abs=2e-12)
    vanilla = VanillaOption(OptionType.Call, 100.0, TODAY + 360, settings)
    vanilla.set_engine(process)
    assert vanilla.npv() == pytest.approx(9.227005508154036, rel=0, abs=2e-12)
