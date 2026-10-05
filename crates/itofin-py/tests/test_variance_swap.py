"""Variance-not-volatility units, finite strips, snapshots and signed payoffs."""

import gc
import math

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import OptionType, Position, VarianceSwap
from itofin.pricingengines import ReplicatingVarianceSwapEngine
from itofin.processes import BlackScholesProcess
from itofin.time import Date, DayCounter

REFERENCE = Date(5, 10, 2026)
CALLS = list(range(100, 151, 5))
PUTS = list(range(50, 101, 5))


def market(position=Position.Long, strike=0.04, notional=50000.0, rate=0.03, dividend=0.0, vol=0.2):
    """Construct a one-year spot-start swap with independent explicit settings."""
    settings = Settings()
    settings.set_evaluation_date(REFERENCE)
    process = BlackScholesProcess(100.0, rate, dividend, vol, REFERENCE, DayCounter.actual365_fixed())
    engine = ReplicatingVarianceSwapEngine(process, CALLS, PUTS)
    swap = VarianceSwap(position, strike, notional, REFERENCE, REFERENCE + 365, settings)
    swap.set_engine(engine)
    return settings, process, engine, swap


def test_static_contract_units_and_lazy_cache():
    """The immutable terms and cached value share annualized variance units."""
    _, _, _, swap = market()
    assert swap.position() == Position.Long
    assert swap.strike() == 0.04
    assert swap.notional() == 50000.0
    assert swap.start_date() == REFERENCE
    assert swap.maturity_date() == REFERENCE + 365
    assert not swap.is_expired()
    assert not swap.is_calculated()
    variance = swap.variance()
    assert swap.is_calculated()
    assert math.isfinite(variance)
    assert swap.npv() == pytest.approx(math.exp(-0.03) * 50000.0 * (variance - 0.04), rel=0, abs=1e-10)
    cached = swap.npv()
    swap.recalculate()
    assert swap.is_calculated()
    assert swap.npv() == cached


def test_short_strike_and_notional_are_payoff_changes_not_vol_inputs():
    """Position negates NPV; strike and notional never change replication."""
    _, _, _, long = market()
    _, _, _, short = market(position=Position.Short)
    _, _, _, bumped = market(strike=0.06)
    _, _, _, scaled = market(notional=100000.0)
    assert short.position() == Position.Short
    assert short.variance() == long.variance() == bumped.variance() == scaled.variance()
    assert short.option_weights() == long.option_weights() == bumped.option_weights() == scaled.option_weights()
    assert short.npv() == -long.npv()
    assert scaled.npv() == 2 * long.npv()
    assert bumped.npv() - long.npv() == pytest.approx(-math.exp(-0.03) * 50000.0 * 0.02, rel=0, abs=1e-10)
    _, _, _, at_fair = market(strike=long.variance())
    assert at_fair.npv() == 0.0


def test_option_weights_are_ordered_typed_fresh_and_detached():
    """Changing either input or output containers cannot mutate the engine."""
    settings, process, _, swap = market()
    calls, puts = CALLS.copy(), PUTS.copy()
    engine = ReplicatingVarianceSwapEngine(process, calls, puts)
    swap.set_engine(engine)
    calls.clear()
    puts[0] = -1
    first = swap.option_weights()
    assert len(first) == len(CALLS) + len(PUTS)
    assert all(isinstance(entry, tuple) and len(entry) == 3 for entry in first)
    assert [entry[0] for entry in first] == [OptionType.Call] * len(CALLS) + [OptionType.Put] * len(PUTS)
    assert [entry[1] for entry in first] == CALLS + PUTS[::-1]
    assert all(math.isfinite(entry[2]) for entry in first)
    original = first.copy()
    first.clear()
    second = swap.option_weights()
    assert second == original
    assert second is not first
    second[0] = (OptionType.Put, -1.0, -2.0)
    assert swap.option_weights() == original
    del engine, process, settings
    gc.collect()
    assert math.isfinite(swap.npv())
    assert swap.option_weights() == original


def test_sorting_deduplication_and_keyword_default_dk():
    """Each side canonicalizes independently and retains its shared boundary."""
    _, process, _, canonical = market()
    settings, _, _, reordered = market()
    engine = ReplicatingVarianceSwapEngine(process, tuple(CALLS[::-1] + [100, 115]), tuple(PUTS[::-1] + [100, 55]))
    reordered.set_engine(engine)
    assert reordered.option_weights() == canonical.option_weights()
    assert reordered.variance() == canonical.variance()
    assert reordered.npv() == canonical.npv()
    explicit = ReplicatingVarianceSwapEngine(process, CALLS, PUTS, dk=5.0)
    reordered.set_engine(explicit)
    assert reordered.npv() == canonical.npv()
    assert settings is not None


def test_zero_volatility_flat_market_hand_case():
    """With zero rate and volatility every purchased option is worthless."""
    _, _, _, swap = market(rate=0.0, vol=0.0)
    assert swap.variance() == 0.0
    assert swap.npv() == -2000.0
    assert len(swap.option_weights()) == 22


@pytest.mark.parametrize("dividend", [0.0, 0.02, -0.01])
def test_wide_fine_strip_constant_market_limit_includes_forward_boundary_mismatch(dividend):
    """The native q term includes the complete boundary-dependent correction."""
    settings, process, _, swap = market(dividend=dividend)
    errors = []
    target = 0.2**2 + 2 * dividend + 2 * (math.exp(0.03 - dividend) - math.exp(0.03))
    for step in (5.0, 2.5, 1.25):
        calls = [100.0 + i * step for i in range(int(200 / step) + 1)]
        puts = [5.0 + i * step for i in range(int(95 / step) + 1)]
        engine = ReplicatingVarianceSwapEngine(process, calls, puts, dk=step / 2)
        swap.set_engine(engine)
        errors.append(abs(swap.variance() - target))
    assert errors[2] < errors[1] < errors[0]
    assert errors[-1] < 4e-5
    assert settings is not None


def test_negative_native_variance_is_not_silently_clamped():
    """A documented dividend/boundary mismatch can produce finite signed output."""
    settings, process, _, swap = market(dividend=0.2)
    swap.set_engine(ReplicatingVarianceSwapEngine(process, list(range(50, 151, 5)), list(range(20, 51, 5))))
    assert math.isfinite(swap.variance())
    assert swap.variance() < 0
    assert swap.npv() < 0
    assert settings is not None


def test_missing_engine_and_settings_raise_without_warming_cache():
    """Constructor ownership does not provide implicit global settings or engine."""
    settings = Settings()
    swap = VarianceSwap(Position.Long, 0.04, 50000.0, REFERENCE, REFERENCE + 365, settings)
    with pytest.raises(ItofinError):
        swap.npv()
    assert not swap.is_calculated()
    settings.set_evaluation_date(REFERENCE)
    with pytest.raises(ItofinError):
        swap.variance()
    assert not swap.is_calculated()
    _, process, _, _ = market()
    swap.set_engine(ReplicatingVarianceSwapEngine(process, CALLS, PUTS))
    assert math.isfinite(swap.npv())
