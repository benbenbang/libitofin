"""Retained quote updates, lazy error recovery and independent pricing checks."""

import gc
import math

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import Position, VarianceSwap
from itofin.pricingengines import ReplicatingVarianceSwapEngine
from itofin.processes import BlackScholesProcess
from itofin.quotes import SimpleQuote
from itofin.termstructures import BlackConstantVol, BlackVarianceSurface, FlatForward
from itofin.time import DayCounter

from test_variance_swap import CALLS, PUTS, REFERENCE


BASE = (0.03, 0.0, 0.2)


def live_market(values=BASE):
    """Bind observable rate, dividend and volatility curves to one process."""
    settings = Settings()
    settings.set_evaluation_date(REFERENCE)
    quotes = [SimpleQuote(value) for value in values]
    dc = DayCounter.actual365_fixed()
    risk_free = FlatForward.from_quote(REFERENCE, quotes[0], dc)
    dividend = FlatForward.from_quote(REFERENCE, quotes[1], dc)
    vol = BlackConstantVol.from_quote(REFERENCE, quotes[2], dc)
    process = BlackScholesProcess.from_curves(100.0, risk_free, dividend, vol)
    engine = ReplicatingVarianceSwapEngine(process, CALLS, PUTS)
    swap = VarianceSwap(Position.Long, 0.04, 50000.0, REFERENCE, REFERENCE + 365, settings)
    swap.set_engine(engine)
    return settings, quotes, process, engine, swap


def black_price(strike, call, rate, dividend, vol, t=1.0):
    """Independently price European payoffs using the normal CDF."""
    forward = 100.0 * math.exp((rate - dividend) * t)
    sigma = vol * math.sqrt(t)
    if sigma == 0:
        return math.exp(-rate * t) * max((forward - strike) * (1 if call else -1), 0.0)
    d1 = math.log(forward / strike) / sigma + sigma / 2
    d2 = d1 - sigma
    sign = 1 if call else -1
    normal1 = math.erfc(-sign * d1 / math.sqrt(2)) / 2
    normal2 = math.erfc(-sign * d2 / math.sqrt(2)) / 2
    return math.exp(-rate * t) * sign * (forward * normal1 - strike * normal2)


def independent_variance(values=BASE):
    """Combine independently computed piecewise payoff slopes and option values."""
    rate, dividend, vol = values
    t, boundary, dk = 1.0, 100.0, 5.0
    cost = 0.0
    for call, strikes, tail in ((True, CALLS, CALLS[-1] + dk), (False, PUTS[::-1], PUTS[0] - dk)):
        previous = 0.0
        extended = strikes + [tail]
        payoff = [2 / t * ((strike - boundary) / boundary - math.log(strike / boundary)) for strike in extended]
        for index, strike in enumerate(strikes):
            slope = abs((payoff[index + 1] - payoff[index]) / (extended[index + 1] - strike))
            weight = slope - previous
            cost += weight * black_price(strike, call, rate, dividend, vol)
            previous = slope
    discount = math.exp(-rate * t)
    return 2 * rate - 2 / t * ((100 / discount - boundary) / boundary + math.log(boundary / 100)) + cost / discount


@pytest.mark.parametrize("index,value", [(0, 0.04), (1, 0.02), (2, 0.25)])
def test_observable_curves_invalidate_and_reprice_without_manual_refresh(index, value):
    """The engine observes its process, intentionally correcting native staleness."""
    _, quotes, _, _, swap = live_market()
    before = swap.npv()
    assert swap.is_calculated()
    quotes[index].set_value(value)
    assert not swap.is_calculated()
    values = list(BASE)
    values[index] = value
    after = swap.npv()
    assert after != before
    expected = independent_variance(values)
    assert swap.variance() == pytest.approx(expected, rel=0, abs=2e-14)
    assert after == pytest.approx(math.exp(-values[0]) * 50000 * (expected - 0.04), rel=0, abs=1e-9)
    _, _, _, _, rebuilt = live_market(values)
    assert after == rebuilt.npv()


@pytest.mark.parametrize("index", [0, 1, 2])
@pytest.mark.parametrize("value", [math.nan, math.inf, -math.inf])
def test_invalid_observable_quote_never_returns_stale_cache_and_recovers(index, value):
    """A failed valuation remains cold and accepts a later valid quote."""
    _, quotes, _, _, swap = live_market()
    original = swap.npv()
    original_variance = swap.variance()
    weights = swap.option_weights()
    quotes[index].set_value(value)
    assert not swap.is_calculated()
    for accessor in (swap.npv, swap.variance, swap.option_weights, swap.recalculate):
        with pytest.raises(ItofinError):
            accessor()
        assert not swap.is_calculated()
    quotes[index].set_value(BASE[index])
    assert swap.npv() == original
    assert swap.variance() == original_variance
    assert swap.option_weights() == weights


def test_engine_process_curves_and_settings_remain_owned_after_python_deletions():
    """Remaining quote edits still invalidate the surviving instrument."""
    settings, quotes, process, engine, swap = live_market()
    expected = swap.npv()
    retained = quotes[2]
    del quotes, process, engine, settings
    gc.collect()
    assert swap.npv() == expected
    retained.set_value(0.25)
    assert not swap.is_calculated()
    assert swap.variance() == pytest.approx(independent_variance((0.03, 0.0, 0.25)), rel=0, abs=2e-14)
    del retained
    gc.collect()
    assert math.isfinite(swap.npv())


def test_reattaching_an_engine_invalidates_prior_results():
    """Replacing the retained market clears the successful instrument cache."""
    _, _, _, _, swap = live_market()
    original = swap.npv()
    _, _, _, replacement, _ = live_market((0.03, 0.0, 0.3))
    swap.set_engine(replacement)
    assert not swap.is_calculated()
    assert swap.npv() != original
    assert swap.variance() == pytest.approx(independent_variance((0.03, 0.0, 0.3)), rel=0, abs=2e-14)


def test_pinned_native_literature_smile_variance_not_volatility():
    """Preserve testReplicatingVarianceSwap's published 1e-4 variance tolerance."""
    settings = Settings()
    settings.set_evaluation_date(REFERENCE)
    dc = DayCounter.actual365_fixed()
    expiry = REFERENCE + 90
    strikes = list(range(50, 136, 5))
    vols = [[0.30 - 0.01 * i] for i in range(len(strikes))]
    surface = BlackVarianceSurface(REFERENCE, [expiry], strikes, vols, dc)
    process = BlackScholesProcess.from_curves(
        100.0, FlatForward(REFERENCE, 0.05, dc), FlatForward(REFERENCE, 0.0, dc), surface
    )
    engine = ReplicatingVarianceSwapEngine(process, list(range(100, 136, 5)), list(range(50, 101, 5)))
    swap = VarianceSwap(Position.Long, 0.04, 50000.0, REFERENCE, expiry, settings)
    swap.set_engine(engine)
    assert swap.variance() == pytest.approx(0.04189, rel=0, abs=1e-4)
    assert len(swap.option_weights()) == 19
    assert swap.npv() == pytest.approx(math.exp(-0.05 * 90 / 365) * 50000 * (swap.variance() - 0.04), rel=0, abs=1e-10)
