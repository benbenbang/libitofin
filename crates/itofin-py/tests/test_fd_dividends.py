"""Cash-dividend FD and analytic dividend European engines (#1370).

REFERENCE SUBSTITUTION (#1369): the FD Spot values are cached from a locally
built C++ QuantLib 9863b578a running the americanoption.cpp:731-761 fixture with
FdBlackScholesVanillaEngine(process, dividends, 100, 400).
"""

import gc
from typing import Any, cast

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.pricingengines import AnalyticDividendEuropeanEngine, FdBlackScholesVanillaEngine
from itofin.processes import BlackScholesProcess
from itofin.time import Date, DayCounter


def _market(today, spot, r, q, vol):
    settings = Settings()
    settings.set_evaluation_date(today)
    return settings, BlackScholesProcess(spot, r, q, vol, today, DayCounter.actual360())


def _option(kind, strike, today, expiry, settings, american):
    if american:
        return VanillaOption.american(kind, strike, today, expiry, settings)
    return VanillaOption(kind, strike, expiry, settings)


TODAY = Date(27, 2, 2021)
MATURITY = Date(27, 2, 2022)
FIRST = Date(27, 12, 2021)
SINGLE = ([FIRST], [10.0])
TWO = ([Date(27, 6, 2021), FIRST], [5.0, 10.0])


@pytest.mark.parametrize(
    ("dividends", "kind", "american", "expected"),
    [
        (SINGLE, OptionType.Call, True, (9.3876035416433616, 0.50439288657051151, 0.014825610960493722)),
        (SINGLE, OptionType.Put, False, (19.549295792091659, -0.57167700814074907, 0.011801370341950067)),
        (TWO, OptionType.Call, True, (7.9057708867966641, 0.49147953278341305, 0.01871370359184878)),
        (TWO, OptionType.Put, False, (22.763590143100142, -0.62808693189317422, 0.011427611391172732)),
    ],
)
def test_spot_cash_dividends_reproduce_cached_quantlib(dividends, kind, american, expected):
    settings, process = _market(TODAY, 100.0, 0.04, 0.08, 0.3)
    dates, amounts = dividends
    engine = FdBlackScholesVanillaEngine(process, 100, 400, dividend_dates=dates, dividend_amounts=amounts)
    option = _option(kind, 100.0, TODAY, MATURITY, settings, american)
    option.set_fd_engine(engine)
    got = (option.npv(), option.delta(), option.gamma())
    for value, want in zip(got, expected, strict=True):
        assert value == pytest.approx(want, abs=1e-8)
    if dividends is SINGLE:
        after = FdBlackScholesVanillaEngine(
            process, 100, 400, dividend_dates=[FIRST, Date(27, 4, 2022)], dividend_amounts=[10.0, 7.0]
        )
        option.set_fd_engine(after)
        assert (option.npv(), option.delta(), option.gamma()) == got


@pytest.mark.parametrize("american", [False, True])
def test_dividend_today_matches_analytic_and_has_no_theta(american):
    """testFdDividendAtTZero (dividendoption.cpp:856-919), Spot arm."""
    today, expiry = Date(27, 2, 2005), Date(13, 4, 2005)
    settings, process = _market(today, 54.625, 0.0, 0.0, 0.282922)
    fd = FdBlackScholesVanillaEngine(process, 50, 400, dividend_dates=[today], dividend_amounts=[1.0])
    analytic = AnalyticDividendEuropeanEngine(process, [today], [1.0])
    expected = VanillaOption(OptionType.Call, 55.0, expiry, settings).price_analytic_dividend(analytic)
    option = _option(OptionType.Call, 55.0, today, expiry, settings, american)
    assert option.price_fd(fd) == pytest.approx(expected, abs=1e-4)
    with pytest.raises(ItofinError):
        option.theta()


def test_analytic_dividend_engine_known_value():
    """testEuropeanKnownValue (dividendoption.cpp:166-221): Hull 12.8."""
    settings, process = _market(TODAY, 40.0, 0.09, 0.0, 0.30)
    engine = AnalyticDividendEuropeanEngine(process, [TODAY + 60, TODAY + 150], [0.5, 0.5])
    expiry = TODAY + 180
    assert VanillaOption(OptionType.Call, 40.0, expiry, settings).price_analytic_dividend(engine) == pytest.approx(
        3.67, abs=1e-2
    )
    american = VanillaOption.american(OptionType.Call, 40.0, TODAY, expiry, settings)
    with pytest.raises(ItofinError, match="not an European option"):
        american.price_analytic_dividend(engine)


def test_invalid_dividend_inputs():
    _, process = _market(TODAY, 100.0, 0.04, 0.08, 0.3)
    with pytest.raises(ItofinError, match="size mismatch"):
        FdBlackScholesVanillaEngine(process, dividend_dates=[FIRST], dividend_amounts=[1.0, 2.0])
    with pytest.raises(ItofinError, match="size mismatch"):
        AnalyticDividendEuropeanEngine(process, [FIRST], [])
    with pytest.raises(ValueError, match="together"):
        FdBlackScholesVanillaEngine(process, dividend_dates=[FIRST])
    with pytest.raises(ValueError, match="together"):
        FdBlackScholesVanillaEngine(process, dividend_amounts=[1.0])
    with pytest.raises(TypeError):
        AnalyticDividendEuropeanEngine(process, cast(Any, [1]), [1.0])


def test_engines_retain_process_and_default_is_unchanged():
    settings, process = _market(TODAY, 100.0, 0.04, 0.08, 0.3)
    plain = FdBlackScholesVanillaEngine(process, 100, 400)
    empty = FdBlackScholesVanillaEngine(process, 100, 400, dividend_dates=[], dividend_amounts=[])
    fd = FdBlackScholesVanillaEngine(process, 100, 400, dividend_dates=[FIRST], dividend_amounts=[10.0])
    analytic = AnalyticDividendEuropeanEngine(process, [FIRST], [10.0])
    del process
    gc.collect()
    for american in (False, True):
        option = _option(OptionType.Put, 100.0, TODAY, MATURITY, settings, american)
        assert option.price_fd(empty) == option.price_fd(plain)
    option = VanillaOption(OptionType.Put, 100.0, MATURITY, settings)
    assert option.price_fd(fd) == pytest.approx(19.549295792091659, abs=1e-8)
    option.set_analytic_dividend_engine(analytic)
    assert option.npv() > 0.0
