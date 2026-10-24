import gc
import math

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.pricingengines import FdBlackScholesVanillaEngine
from itofin.processes import BlackScholesProcess
from itofin.time import Date, DayCounter


@pytest.mark.parametrize("strike", [-1.0, math.nan, math.inf, -math.inf])
def test_asset_digital_factory_invalid_strikes(strike):
    with pytest.raises(ValueError, match="finite and nonnegative"):
        VanillaOption.asset_or_nothing(OptionType.Call, strike, Date(15, 6, 2027), Settings())


def test_asset_digital_fd_fields_expiry_and_lifetime():
    today = Date(15, 6, 2026)
    settings = Settings()
    settings.set_evaluation_date(today)
    process = BlackScholesProcess(100.0, 0.05, 0.02, 0.2, today, DayCounter.actual360())
    engine = FdBlackScholesVanillaEngine(process)
    for kind in [OptionType.Call, OptionType.Put]:
        option = VanillaOption.asset_or_nothing(kind, 105.0, today + 360, settings)
        with pytest.raises(ItofinError):
            option.npv()
        with pytest.raises(ItofinError, match="payoff"):
            option.price(process)
        option.set_fd_engine(engine)
        values = [option.npv(), option.delta(), option.gamma(), option.theta()]
        assert all(math.isfinite(value) for value in values)
        assert 0 < values[0] < 100
        for query in [option.vega, option.rho, option.dividend_rho, option.error_estimate]:
            with pytest.raises(ItofinError):
                query()
        settings.set_evaluation_date(today + 360)
        assert option.npv() == 0
        settings.set_evaluation_date(today)
        assert option.npv() == values[0]
    option = VanillaOption.asset_or_nothing(OptionType.Call, 100.0, today + 360, settings)
    option.set_fd_engine(engine)
    del settings, process, engine
    gc.collect()
    assert math.isfinite(option.npv()) and option.npv() > 0
