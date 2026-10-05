"""Variance-swap constructor bounds, unsupported dates and atomic cache errors."""

import math

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import Position, VarianceSwap
from itofin.pricingengines import ReplicatingVarianceSwapEngine
from itofin.processes import BlackScholesProcess
from itofin.time import Date, DayCounter

from test_variance_swap import CALLS, PUTS, REFERENCE, market


@pytest.mark.parametrize("value", [0.0, -1.0, math.inf, -math.inf, math.nan])
@pytest.mark.parametrize("term", ["strike", "notional"])
def test_strike_and_notional_must_be_finite_positive(term, value):
    """Positive variance units reject IEEE special values explicitly."""
    settings = Settings()
    values = {"strike": 0.04, "notional": 50000.0, term: value}
    with pytest.raises(ItofinError):
        VarianceSwap(Position.Long, values["strike"], values["notional"], REFERENCE, REFERENCE + 365, settings)


@pytest.mark.parametrize("days", [-1, 0])
def test_start_must_precede_maturity(days):
    """Invalid contract windows are rejected at construction."""
    with pytest.raises(ItofinError):
        VarianceSwap(Position.Long, 0.04, 50000.0, REFERENCE, REFERENCE + days, Settings())


@pytest.mark.parametrize("days", [-1, 1])
def test_forward_and_seasoned_live_starts_are_rejected(days):
    """No realized fixings or forward-start subtraction are silently invented."""
    settings, _, engine, _ = market()
    swap = VarianceSwap(Position.Long, 0.04, 50000.0, REFERENCE + days, REFERENCE + 365, settings)
    swap.set_engine(engine)
    for accessor in (swap.npv, swap.variance, swap.option_weights, swap.recalculate):
        with pytest.raises(ItofinError):
            accessor()
        assert not swap.is_calculated()


@pytest.mark.parametrize("dk", [0.0, -1.0, math.inf, -math.inf, math.nan, 50.0, 1e-300])
def test_dk_must_have_positive_representable_tail_gaps(dk):
    """A zero lower tail and rounded-away gaps are both unsupported."""
    _, process, _, _ = market()
    with pytest.raises(ItofinError):
        ReplicatingVarianceSwapEngine(process, CALLS, PUTS, dk=dk)


@pytest.mark.parametrize(
    "calls,puts",
    [
        ([], PUTS),
        (CALLS, []),
        ([100.0], PUTS),
        (CALLS, [100.0]),
        ([100.0, 100.0], PUTS),
        (CALLS, [100.0, 100.0]),
        ([100.0] * 4097, PUTS),
        (CALLS, [100.0] * 4097),
        ([101.0, 105.0], PUTS),
        ([0.0, 100.0], PUTS),
        (CALLS, [-1.0, 100.0]),
        ([100.0, math.inf], PUTS),
        (CALLS, [math.nan, 100.0]),
        ([100.0, 1e308], PUTS),
    ],
)
def test_inadequate_mismatched_nonfinite_and_overflowing_strips(calls, puts):
    """Raw bounds precede core copies; canonical strips must be adequate."""
    _, process, _, _ = market()
    dk = 1e308 if calls == [100.0, 1e308] else 5.0
    with pytest.raises(ItofinError):
        ReplicatingVarianceSwapEngine(process, calls, puts, dk=dk)


@pytest.mark.parametrize("leg", ["risk_free", "dividend", "vol"])
def test_every_market_reference_date_must_equal_supported_start(leg):
    """The retained market cannot hide inconsistent accrual origins."""
    from itofin.termstructures import BlackConstantVol, FlatForward

    settings, _, _, swap = market()
    dc = DayCounter.actual365_fixed()
    risk_free = FlatForward(REFERENCE + (1 if leg == "risk_free" else 0), 0.03, dc)
    dividend = FlatForward(REFERENCE + (1 if leg == "dividend" else 0), 0.0, dc)
    vol = BlackConstantVol(REFERENCE + (1 if leg == "vol" else 0), 0.2, dc)
    process = BlackScholesProcess.from_curves(100.0, risk_free, dividend, vol)
    swap.set_engine(ReplicatingVarianceSwapEngine(process, CALLS, PUTS))
    with pytest.raises(ItofinError):
        swap.npv()
    assert not swap.is_calculated()
    assert settings is not None


def test_differing_curve_day_counters_are_supported():
    """Risk-free annualization does not force volatility's own day count."""
    from itofin.termstructures import BlackConstantVol, FlatForward

    settings, _, _, swap = market()
    risk_free = FlatForward(REFERENCE, 0.03, DayCounter.actual365_fixed())
    dividend = FlatForward(REFERENCE, 0.0, DayCounter.actual360())
    vol = BlackConstantVol(REFERENCE, 0.2, DayCounter.actual360())
    process = BlackScholesProcess.from_curves(100.0, risk_free, dividend, vol)
    swap.set_engine(ReplicatingVarianceSwapEngine(process, CALLS, PUTS))
    assert math.isfinite(swap.npv())
    assert math.isfinite(swap.variance())
    assert settings is not None


def test_moving_evaluation_date_rejects_seasoning_then_recovers():
    """A valid cached spot start must not survive an unsupported date move."""
    settings, _, _, swap = market()
    original = swap.npv()
    settings.set_evaluation_date(REFERENCE + 1)
    assert not swap.is_calculated()
    with pytest.raises(ItofinError):
        swap.npv()
    assert not swap.is_calculated()
    settings.set_evaluation_date(REFERENCE)
    assert swap.npv() == original


def test_maturity_day_default_expiry_and_missing_expired_extra_results():
    """Expiry pays zero without requiring a supported live start or market."""
    settings, _, _, swap = market()
    swap.npv()
    settings.set_evaluation_date(REFERENCE + 365)
    assert swap.is_expired()
    assert swap.npv() == 0.0
    for accessor in (swap.variance, swap.option_weights):
        with pytest.raises(ItofinError):
            accessor()
    settings.set_evaluation_date(REFERENCE + 366)
    assert swap.is_expired()
    assert swap.npv() == 0.0


def test_new_unsendable_class_modules_are_public_and_stable():
    """Both concrete classes advertise their established package namespace."""
    assert VarianceSwap.__module__ == "itofin.instruments"
    assert ReplicatingVarianceSwapEngine.__module__ == "itofin.pricingengines"
    assert Date is not None


@pytest.mark.parametrize("spot", [0.0, -1.0, math.nan, math.inf, -math.inf])
def test_invalid_spot_cannot_publish_finite_cached_results(spot):
    """Scalar market construction does not bypass the engine's input checks."""
    settings, _, _, swap = market()
    process = BlackScholesProcess(spot, 0.03, 0.0, 0.2, REFERENCE, DayCounter.actual365_fixed())
    swap.set_engine(ReplicatingVarianceSwapEngine(process, CALLS, PUTS))
    for accessor in (swap.npv, swap.variance, swap.option_weights):
        with pytest.raises(ItofinError):
            accessor()
        assert not swap.is_calculated()
    assert settings is not None


def test_maximum_raw_strip_size_counts_before_deduplication():
    """The exact 4096-input bound permits duplicates but does not hide overflow."""
    _, process, _, canonical = market()
    calls = CALLS + [100.0] * (4096 - len(CALLS))
    puts = PUTS + [100.0] * (4096 - len(PUTS))
    canonical_weights = canonical.option_weights()
    canonical.set_engine(ReplicatingVarianceSwapEngine(process, calls, puts))
    assert canonical.option_weights() == canonical_weights


def test_dk_is_keyword_only_and_wrong_engine_type_does_not_mutate_cache():
    """Rejected Python argument extraction leaves the attached engine intact."""
    _, process, _, swap = market()
    expected = swap.npv()
    with pytest.raises(TypeError):
        getattr(ReplicatingVarianceSwapEngine, "__new__")(ReplicatingVarianceSwapEngine, process, CALLS, PUTS, 5.0)
    with pytest.raises(TypeError):
        getattr(swap, "set_engine")(process)
    assert swap.is_calculated()
    assert swap.npv() == expected
