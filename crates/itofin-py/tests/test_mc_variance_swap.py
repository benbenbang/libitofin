"""Annualized integrated diffusion variance, signed cash error and bounded MC."""

import math

import pytest

from itofin import Settings
from itofin.instruments import Position, VarianceSwap
from itofin.pricingengines import MCVarianceSwapEngine, ReplicatingVarianceSwapEngine
from itofin.processes import BlackScholesProcess
from itofin.termstructures import BlackVarianceCurve, FlatForward
from itofin.time import Date, DayCounter

REFERENCE = Date(6, 10, 2026)


def market(position=Position.Long, vol=0.2, rate=0.03, dividend=0.0, **options):
    """Create isolated explicit settings and a spot-start one-year swap."""
    settings = Settings()
    settings.set_evaluation_date(REFERENCE)
    process = BlackScholesProcess(100.0, rate, dividend, vol, REFERENCE, DayCounter.actual365_fixed())
    configuration = {"steps": 12, "samples": 32, "seed": 42}
    configuration.update(options)
    engine = MCVarianceSwapEngine(process, **configuration)
    swap = VarianceSwap(position, 0.04, 50000.0, REFERENCE, REFERENCE + 365, settings)
    swap.set_engine(engine)
    return settings, process, engine, swap


def curve_market(position=Position.Long, **options):
    """Preserve native testMCVarianceSwap's independent curve and sampling."""
    settings = Settings()
    settings.set_evaluation_date(REFERENCE)
    dc = DayCounter.actual365_fixed()
    maturity = REFERENCE + 90
    curve = BlackVarianceCurve(REFERENCE, [REFERENCE + 36, maturity], [0.1, 0.2], dc, True)
    process = BlackScholesProcess.from_curves(
        100.0, FlatForward(REFERENCE, 0.05, dc), FlatForward(REFERENCE, 0.0, dc), curve
    )
    configuration = {"steps_per_year": 250, "samples": 1023, "seed": 42}
    configuration.update(options)
    engine = MCVarianceSwapEngine(process, **configuration)
    swap = VarianceSwap(position, 0.04, 50000.0, REFERENCE, maturity, settings)
    swap.set_engine(engine)
    return settings, process, engine, swap


@pytest.mark.parametrize("vol", [0.0, 0.1, 0.2, 0.5])
@pytest.mark.parametrize("steps", [1, 12, 252])
def test_flat_diffusion_estimator_is_variance_not_squared_realized_returns(vol, steps):
    """Even a one-step path integrates a flat sigma squared exactly."""
    _, _, _, swap = market(vol=vol, steps=steps)
    assert not swap.is_calculated()
    assert swap.variance() == pytest.approx(vol * vol, rel=0, abs=2e-14)
    assert swap.npv() == pytest.approx(math.exp(-0.03) * 50000 * (vol * vol - 0.04), rel=0, abs=2e-9)
    assert swap.is_calculated()
    assert swap.variance_error() == pytest.approx(0.0, rel=0, abs=2e-14)
    assert swap.samples() == 32
    assert swap.option_weights() == []
    assert math.isfinite(swap.error_estimate())


def test_pinned_native_time_dependent_curve_keeps_original_statistical_band():
    """The 36/90-day curve uses 250 steps/year, 1023 paths and seed42."""
    _, _, _, swap = curve_market()
    assert swap.variance() == pytest.approx(0.04, rel=0, abs=3e-4)
    assert swap.samples() == 1023
    assert swap.variance_error() >= 0.0
    assert swap.error_estimate() == math.exp(-0.05 * 90 / 365) * 50000 * swap.variance_error()
    assert swap.option_weights() == []


def test_native_signed_cash_error_is_distinct_from_nonnegative_variance_error():
    """Position negates NPV/error estimate but not variance, error or count."""
    _, _, _, long = curve_market()
    _, _, _, short = curve_market(position=Position.Short)
    assert short.variance() == long.variance()
    assert short.variance_error() == long.variance_error()
    assert short.variance_error() >= 0.0
    assert short.samples() == long.samples()
    assert short.npv() == -long.npv()
    assert short.error_estimate() == -long.error_estimate()
    assert math.copysign(1.0, short.error_estimate()) == -1.0


def test_fixed_seed_recalculation_starts_from_same_generator():
    """Repeated seeded valuation cannot accumulate another prior-run batch."""
    _, _, _, swap = curve_market()
    expected = (swap.variance(), swap.npv(), swap.variance_error(), swap.error_estimate(), swap.samples())
    for _ in range(3):
        swap.recalculate()
        assert (swap.variance(), swap.npv(), swap.variance_error(), swap.error_estimate(), swap.samples()) == expected


def test_absolute_variance_tolerance_reports_actual_observations():
    """A flat estimator reaches tolerance in the native initial 1023 batch."""
    _, _, _, swap = market(samples=None, tolerance=1e-6, max_samples=2046)
    assert swap.variance() == pytest.approx(0.04, rel=0, abs=2e-14)
    assert swap.variance_error() <= 1e-6
    assert swap.samples() == 1023
    _, _, _, default_cap = market(samples=None, tolerance=1e-6)
    assert default_cap.samples() == 1023


def test_grid_selectors_are_equivalent_at_exact_one_year():
    """The annualized grid selector uses the same native grid as explicit steps."""
    _, _, _, explicit = market(steps=24)
    _, _, _, annualized = market(steps=None, steps_per_year=24)
    assert explicit.variance() == annualized.variance()
    assert explicit.samples() == annualized.samples()


def test_zero_seed_uses_randomized_convention_without_promising_reproducibility():
    """Zero seed is accepted without a false deterministic-stream guarantee."""
    _, _, _, swap = market(seed=0)
    assert swap.variance() == pytest.approx(0.04, rel=0, abs=2e-14)
    swap.recalculate()
    assert swap.samples() == 32


def test_dense_zero_dividend_replication_approaches_mc_flat_limit():
    """Compare a valid strip limit without hiding finite-strip/grid bias."""
    _, process, _, swap = market()
    target = swap.variance()
    errors = []
    for step in (5.0, 2.5, 1.25):
        calls = [100.0 + i * step for i in range(int(200 / step) + 1)]
        puts = [5.0 + i * step for i in range(int(95 / step) + 1)]
        swap.set_engine(ReplicatingVarianceSwapEngine(process, calls, puts, dk=step / 2))
        errors.append(abs(swap.variance() - target))
    assert errors[2] < errors[1] < errors[0]
    assert errors[-1] < 4e-5


def test_mc_class_uses_established_pricing_engine_namespace():
    """The new unsendable class advertises the same namespace as replication."""
    assert MCVarianceSwapEngine.__module__ == "itofin.pricingengines"


def test_fixed_samples_respect_an_explicit_larger_sample_budget():
    """A maximum can bound fixed mode but does not request extra observations."""
    _, _, _, swap = market(max_samples=1023)
    assert swap.samples() == 32
    assert swap.variance() == pytest.approx(0.04, rel=0, abs=2e-14)
