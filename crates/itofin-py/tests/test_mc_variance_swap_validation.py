"""MC option selection, resource limits, start scope and finite validation."""

import math
from typing import Any, cast

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import Position, VarianceSwap
from itofin.pricingengines import MCVarianceSwapEngine
from itofin.processes import BlackScholesProcess
from itofin.termstructures import BlackConstantVol, BlackVarianceSurface, FlatForward
from itofin.time import DayCounter

from test_mc_variance_swap import REFERENCE, curve_market, market


@pytest.mark.parametrize(
    "options",
    [
        {},
        {"steps": 12},
        {"samples": 32},
        {"steps": 12, "steps_per_year": 12, "samples": 32},
        {"steps": 0, "samples": 32},
        {"steps_per_year": 0, "samples": 32},
        {"steps": 12, "samples": 0},
        {"steps": 12, "samples": 1},
        {"steps": 12, "samples": 32, "tolerance": 1e-6},
        {"steps": 12, "samples": 32, "max_samples": 31},
        {"steps": 12, "tolerance": 0.0},
        {"steps": 12, "tolerance": -1.0},
        {"steps": 12, "tolerance": math.nan},
        {"steps": 12, "tolerance": math.inf},
        {"steps": 12, "tolerance": -math.inf},
        {"steps": 12, "tolerance": 1e-6, "max_samples": 0},
        {"steps": 12, "tolerance": 1e-6, "max_samples": 1022},
        {"steps": 100001, "samples": 2},
        {"steps_per_year": 100001, "samples": 2},
        {"steps": 12, "samples": 1000001},
        {"steps": 12, "tolerance": 1e-6, "max_samples": 1000001},
        {"steps": 100000, "samples": 1001},
        {"steps": 12, "samples": 32, "seed": 2**32},
    ],
)
def test_invalid_selectors_budgets_and_total_work_are_rejected(options):
    """No ignored selector, unbounded iteration or failed-budget result."""
    _, process, _, swap = market()
    with pytest.raises(ItofinError):
        engine = MCVarianceSwapEngine(process, **options)
        swap.set_engine(engine)
        swap.npv()


@pytest.mark.parametrize("field", ["steps", "steps_per_year", "samples", "max_samples", "seed"])
def test_negative_unsigned_arguments_raise_at_python_boundary(field):
    """Negative grid/count/seed values cannot wrap into native unsigned input."""
    _, process, _, _ = market()
    options = {"steps": 12, "samples": 32, "seed": 42, field: -1}
    with pytest.raises(OverflowError):
        MCVarianceSwapEngine(process, **options)


@pytest.mark.parametrize("feature", ["antithetic", "brownian_bridge", "control_variate", "quasi_mc"])
def test_advanced_mc_options_are_not_silently_accepted(feature):
    """PseudoRandom-only configuration cannot enable unimplemented extras."""
    _, process, _, _ = market()
    with pytest.raises(TypeError):
        MCVarianceSwapEngine(process, steps=12, samples=32, **{feature: True})


def test_keyword_only_configuration_and_unsupported_process_type():
    """A named arithmetic GBM or another object is not a log-price process."""
    _, process, _, _ = market()
    with pytest.raises(TypeError):
        getattr(MCVarianceSwapEngine, "__new__")(MCVarianceSwapEngine, process, 12)
    with pytest.raises(TypeError):
        MCVarianceSwapEngine(cast(Any, object()), steps=12, samples=32)


@pytest.mark.parametrize("offset", [-1, 1])
def test_historical_and_forward_start_contracts_are_not_mc_fixing_models(offset):
    """Both unsupported histories and future starts fail without warm cache."""
    settings, process, _, _ = market()
    swap = VarianceSwap(Position.Long, 0.04, 50000.0, REFERENCE + offset, REFERENCE + 365, settings)
    swap.set_engine(MCVarianceSwapEngine(process, steps=12, samples=32, seed=42))
    with pytest.raises(ItofinError):
        swap.npv()
    assert not swap.is_calculated()


@pytest.mark.parametrize("leg", ["risk_free", "dividend", "vol"])
def test_all_market_reference_dates_must_match_contract_start(leg):
    """A retained curve cannot conceal an inconsistent accrual origin."""
    _, _, _, swap = market()
    dc = DayCounter.actual365_fixed()
    process = BlackScholesProcess.from_curves(
        100.0,
        FlatForward(REFERENCE + (leg == "risk_free"), 0.03, dc),
        FlatForward(REFERENCE + (leg == "dividend"), 0.0, dc),
        BlackConstantVol(REFERENCE + (leg == "vol"), 0.2, dc),
    )
    swap.set_engine(MCVarianceSwapEngine(process, steps=12, samples=32, seed=42))
    with pytest.raises(ItofinError):
        swap.variance()
    assert not swap.is_calculated()


@pytest.mark.parametrize("spot", [0.0, -1.0, math.nan, math.inf, -math.inf])
def test_invalid_log_price_process_state_is_not_published(spot):
    """Every state must be finite and positive before log-price evolution."""
    _, _, _, swap = market()
    process = BlackScholesProcess(spot, 0.03, 0.0, 0.2, REFERENCE, DayCounter.actual365_fixed())
    swap.set_engine(MCVarianceSwapEngine(process, steps=12, samples=32, seed=42))
    with pytest.raises(ItofinError):
        swap.npv()
    assert not swap.is_calculated()


def test_unsupported_black_surface_does_not_implicitly_add_local_vol_surface():
    """Only the documented constant and linear variance-curve dispatch exists."""
    _, _, _, swap = market()
    dc = DayCounter.actual365_fixed()
    surface = BlackVarianceSurface(REFERENCE, [REFERENCE + 365], [90.0, 110.0], [[0.2], [0.2]], dc)
    process = BlackScholesProcess.from_curves(
        100.0, FlatForward(REFERENCE, 0.03, dc), FlatForward(REFERENCE, 0.0, dc), surface
    )
    with pytest.raises(ItofinError):
        engine = MCVarianceSwapEngine(process, steps=12, samples=32, seed=42)
        swap.set_engine(engine)
        swap.variance()


def test_missing_settings_or_engine_cannot_publish_mc_statistics():
    """Additional getters use the same lazy validation as the inherited NPV."""
    settings = Settings()
    swap = VarianceSwap(Position.Long, 0.04, 50000.0, REFERENCE, REFERENCE + 365, settings)
    for accessor in (swap.npv, swap.variance, swap.variance_error, swap.samples, swap.error_estimate):
        with pytest.raises(ItofinError):
            accessor()
        assert not swap.is_calculated()
    settings.set_evaluation_date(REFERENCE)
    for accessor in (swap.variance_error, swap.samples, swap.error_estimate):
        with pytest.raises(ItofinError):
            accessor()
        assert not swap.is_calculated()


def test_tolerance_budget_exhaustion_never_publishes_unconverged_results():
    """The pinned curve has tiny roundoff error, below normal useful tolerance."""
    _, _, _, pilot = curve_market()
    error = pilot.variance_error()
    assert error > 0.0
    _, process, _, swap = curve_market()
    exhausted = MCVarianceSwapEngine(process, steps_per_year=250, tolerance=error / 2, max_samples=1023, seed=42)
    swap.set_engine(exhausted)
    for accessor in (swap.npv, swap.variance, swap.variance_error, swap.error_estimate, swap.samples):
        with pytest.raises(ItofinError):
            accessor()
        assert not swap.is_calculated()
    swap.set_engine(MCVarianceSwapEngine(process, steps_per_year=250, samples=1023, seed=42))
    assert swap.variance() == pilot.variance()
    assert swap.samples() == 1023
