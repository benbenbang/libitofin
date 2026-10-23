import math

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.processes import BlackScholesProcess
from itofin.time import Date, DayCounter


def test_cash_digital_analytic_price_and_greeks():
    today = Date(15, 6, 2026)
    settings = Settings()
    settings.set_evaluation_date(today)
    process = BlackScholesProcess(100.0, 0.05, 0.02, 0.2, today, DayCounter.actual360())
    d2 = (math.log(100.0 / 105.0) + 0.03 - 0.02) / 0.2
    discount = math.exp(-0.05)
    density = math.exp(-d2 * d2 / 2) / math.sqrt(2 * math.pi)
    for kind, sign in [(OptionType.Call, 1), (OptionType.Put, -1)]:
        option = VanillaOption.cash_or_nothing(kind, 105.0, 12.0, today + 360, settings)
        with pytest.raises(ItofinError):
            option.npv()
        option.set_engine(process)
        value = 12 * discount * math.erfc(-sign * d2 / math.sqrt(2)) / 2
        delta = sign * 12 * discount * density / (100 * 0.2)
        assert option.npv() == pytest.approx(value, rel=0, abs=2e-12)
        assert option.delta() == pytest.approx(delta, rel=0, abs=2e-12)
        gamma = -delta / 100 * (1 + d2 / 0.2)
        assert option.gamma() == pytest.approx(gamma, rel=0, abs=2e-12)
        theta = 0.05 * value - 0.03 * 100 * delta - 0.5 * 0.2**2 * 100**2 * gamma
        assert option.theta() == pytest.approx(theta, rel=0, abs=2e-12)
        assert option.rho() == pytest.approx(100 * delta - value, rel=0, abs=2e-12)
        assert option.dividend_rho() == pytest.approx(-100 * delta, rel=0, abs=2e-12)
        assert option.vega() == pytest.approx(-100 * delta * (d2 + 0.2), rel=0, abs=2e-12)


@pytest.mark.parametrize(
    "strike,cash", [(-1.0, 1.0), (math.nan, 1.0), (math.inf, 1.0), (1.0, math.nan), (1.0, math.inf)]
)
def test_cash_digital_rejects_invalid_parameters(strike, cash):
    with pytest.raises(ValueError):
        VanillaOption.cash_or_nothing(OptionType.Call, strike, cash, Date(15, 6, 2027), Settings())
