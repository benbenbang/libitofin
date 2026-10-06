"""Ownership, observers, settings and engine replacement preserve MC results."""

import gc
import math

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import Position, VarianceSwap
from itofin.pricingengines import MCVarianceSwapEngine, ReplicatingVarianceSwapEngine
from itofin.processes import BlackScholesProcess
from itofin.quotes import SimpleQuote
from itofin.termstructures import BlackConstantVol, FlatForward
from itofin.time import DayCounter

from test_mc_variance_swap import REFERENCE, market

BASE = (0.03, 0.0, 0.2)


def live_market(values=BASE):
    """Hold market quotes separately from all retained Python pricing owners."""
    settings = Settings()
    settings.set_evaluation_date(REFERENCE)
    quotes = [SimpleQuote(value) for value in values]
    dc = DayCounter.actual365_fixed()
    process = BlackScholesProcess.from_curves(
        100.0,
        FlatForward.from_quote(REFERENCE, quotes[0], dc),
        FlatForward.from_quote(REFERENCE, quotes[1], dc),
        BlackConstantVol.from_quote(REFERENCE, quotes[2], dc),
    )
    engine = MCVarianceSwapEngine(process, steps=12, samples=32, seed=42)
    swap = VarianceSwap(Position.Long, 0.04, 50000.0, REFERENCE, REFERENCE + 365, settings)
    swap.set_engine(engine)
    return settings, quotes, process, engine, swap


@pytest.mark.parametrize("index,value", [(0, 0.04), (1, 0.02), (2, 0.25)])
def test_live_inputs_invalidate_and_reproduce_reconstructed_market(index, value):
    """Observer correction reprices quotes without forcing explicit recalculate."""
    _, quotes, _, _, swap = live_market()
    swap.npv()
    assert swap.is_calculated()
    quotes[index].set_value(value)
    assert not swap.is_calculated()
    values = list(BASE)
    values[index] = value
    _, _, _, _, rebuilt = live_market(values)
    assert swap.variance() == rebuilt.variance()
    assert swap.npv() == rebuilt.npv()
    assert swap.variance_error() == rebuilt.variance_error()
    assert swap.error_estimate() == rebuilt.error_estimate()
    assert swap.samples() == rebuilt.samples()


@pytest.mark.parametrize("index", [0, 1, 2])
@pytest.mark.parametrize("value", [math.nan, math.inf, -math.inf])
def test_failed_live_input_returns_no_stale_or_partial_results_and_recovers(index, value):
    """Failed estimates stay cold and restoration restarts the seeded stream."""
    _, quotes, _, _, swap = live_market()
    expected = (swap.npv(), swap.variance(), swap.variance_error(), swap.error_estimate(), swap.samples())
    quotes[index].set_value(value)
    for accessor in (swap.npv, swap.variance, swap.variance_error, swap.error_estimate, swap.samples):
        with pytest.raises(ItofinError):
            accessor()
        assert not swap.is_calculated()
    quotes[index].set_value(BASE[index])
    assert (swap.npv(), swap.variance(), swap.variance_error(), swap.error_estimate(), swap.samples()) == expected


def test_deleted_process_engine_settings_and_curves_remain_owned_by_swap():
    """Only the quote and instrument survive, yet live edits still propagate."""
    settings, quotes, process, engine, swap = live_market()
    expected = swap.variance()
    retained = quotes[2]
    del quotes, process, engine, settings
    gc.collect()
    assert swap.variance() == expected
    retained.set_value(0.25)
    assert not swap.is_calculated()
    assert swap.variance() == pytest.approx(0.25**2, rel=0, abs=2e-14)
    assert swap.samples() == 32
    del retained
    gc.collect()
    swap.recalculate()
    assert math.isfinite(swap.npv())


def test_replacement_resets_statistics_between_replication_and_mc():
    """Results from a detached engine cannot bleed into a different engine."""
    _, process, engine, swap = market()
    assert swap.samples() == 32
    assert swap.option_weights() == []
    replication = ReplicatingVarianceSwapEngine(process, [100.0, 110.0], [90.0, 100.0])
    swap.set_engine(replication)
    assert not swap.is_calculated()
    assert len(swap.option_weights()) == 4
    for accessor in (swap.variance_error, swap.error_estimate, swap.samples):
        with pytest.raises(ItofinError):
            accessor()
    swap.set_engine(engine)
    assert not swap.is_calculated()
    assert swap.samples() == 32
    assert swap.option_weights() == []
    assert swap.variance_error() >= 0.0


def test_wrong_engine_type_leaves_successful_attached_engine_unchanged():
    """Failed Python union extraction must precede mutating the core engine."""
    _, process, _, swap = market()
    expected = swap.npv()
    with pytest.raises(TypeError):
        getattr(swap, "set_engine")(process)
    assert swap.is_calculated()
    assert swap.npv() == expected
    assert swap.samples() == 32


def test_settings_seasoning_failure_recovers_and_expiry_reports_native_cash_zero():
    """Past live starts are unsupported; expired extras are unavailable."""
    settings, _, _, swap = market()
    expected = swap.npv()
    settings.set_evaluation_date(REFERENCE + 1)
    assert not swap.is_calculated()
    for accessor in (swap.npv, swap.variance, swap.samples, swap.variance_error, swap.error_estimate):
        with pytest.raises(ItofinError):
            accessor()
        assert not swap.is_calculated()
    settings.set_evaluation_date(REFERENCE)
    assert swap.npv() == expected
    settings.set_evaluation_date(REFERENCE + 365)
    assert swap.is_expired()
    assert swap.npv() == 0.0
    assert swap.error_estimate() == 0.0
    for accessor in (swap.variance, swap.variance_error, swap.samples, swap.option_weights):
        with pytest.raises(ItofinError):
            accessor()
