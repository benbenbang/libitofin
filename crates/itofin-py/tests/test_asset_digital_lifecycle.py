import gc
import math

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.models import HestonModel
from itofin.pricingengines import FdBlackScholesVanillaEngine, MCEuropeanEngine
from itofin.processes import BlackScholesProcess, HestonProcess
from itofin.quotes import SimpleQuote
from itofin.termstructures import BlackConstantVol, FlatForward
from itofin.time import Date, DayCounter


def test_asset_digital_live_curve_volatility_and_retained_graph():
    today = Date(15, 6, 2026)
    settings = Settings()
    settings.set_evaluation_date(today)
    dc = DayCounter.actual360()
    quotes = [SimpleQuote(value) for value in [0.05, 0.02, 0.2]]
    risk = FlatForward.from_quote(today, quotes[0], dc)
    dividend = FlatForward.from_quote(today, quotes[1], dc)
    vol = BlackConstantVol.from_quote(today, quotes[2], dc)
    process = BlackScholesProcess.from_curves(100.0, risk, dividend, vol)
    engine = FdBlackScholesVanillaEngine(process)
    option = VanillaOption.asset_or_nothing(OptionType.Call, 105.0, today + 360, settings)
    before = option.price_fd(engine)
    assert math.isfinite(before)
    snapshot = option.results()
    del process, engine, risk, dividend, vol
    gc.collect()
    values = [0.05, 0.02, 0.2]
    previous = before
    for index, value in enumerate([0.07, 0.04, 0.3]):
        quotes[index].set_value(value)
        values[index] = value
        assert not option.is_calculated()
        fresh_process = BlackScholesProcess(100.0, values[0], values[1], values[2], today, dc)
        fresh_engine = FdBlackScholesVanillaEngine(fresh_process)
        fresh = VanillaOption.asset_or_nothing(OptionType.Call, 105.0, today + 360, settings)
        want = fresh.price_fd(fresh_engine)
        got = option.npv()
        assert math.isfinite(got) and math.isfinite(want)
        assert abs(got - want) <= 1e-10
        assert abs(got - previous) > 1e-4
        previous = got
    assert snapshot.npv == before
    settings.set_evaluation_date(today + 360)
    assert option.npv() == 0
    settings.set_evaluation_date(today)
    del settings, quotes, dc, fresh, fresh_engine, fresh_process
    gc.collect()
    assert math.isfinite(option.npv()) and abs(option.npv() - previous) <= 1e-10


def test_asset_digital_unsupported_engines_fail_and_fd_recovers():
    today = Date(15, 6, 2026)
    dc = DayCounter.actual360()
    settings = Settings()
    settings.set_evaluation_date(today)
    process = BlackScholesProcess(100.0, 0.05, 0.02, 0.2, today, dc)
    option = VanillaOption.asset_or_nothing(OptionType.Put, 100.0, today + 360, settings)
    with pytest.raises(ItofinError, match="payoff"):
        option.price(process)
    option.set_mc_engine(MCEuropeanEngine(process, steps=1, samples=100))
    with pytest.raises(ItofinError, match="payoff"):
        option.npv()
    heston = HestonModel(HestonProcess(0.05, 0.02, 100.0, 0.04, 1.0, 0.04, 0.3, -0.5, today, dc))
    option.set_heston_engine(heston, 64)
    with pytest.raises(ItofinError, match="plain"):
        option.npv()
    engine = FdBlackScholesVanillaEngine(process)
    assert math.isfinite(option.price_fd(engine)) and option.npv() > 0
    for spot in [0.0, -1.0, math.nan]:
        invalid = BlackScholesProcess(spot, 0.05, 0.02, 0.2, today, dc)
        with pytest.raises(ItofinError):
            option.price_fd(FdBlackScholesVanillaEngine(invalid))
    assert math.isfinite(option.price_fd(engine)) and option.npv() > 0
