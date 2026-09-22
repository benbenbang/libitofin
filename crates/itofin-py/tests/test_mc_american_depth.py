"""LSM basis selection and retained American/Bermudan contracts."""

import gc
import math

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import BermudanExercise, OptionType, VanillaOption
from itofin.pricingengines import MCAmericanEngine
from itofin.processes import BlackScholesProcess
from itofin.quotes import SimpleQuote
from itofin.termstructures import BlackConstantVol, FlatForward
from itofin.time import Date, DayCounter

TODAY = Date(17, 5, 1998)
EXPIRY = Date(17, 5, 1999)


def _engine(process, basis=0):
    return MCAmericanEngine(
        process,
        steps=25,
        samples=2048,
        seed=42,
        antithetic=True,
        polynomial_order=3,
        calibration_samples=2048,
        basis_system=basis,
    )


def _market(rate=0.06):
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    process = BlackScholesProcess(36.0, rate, 0.0, 0.2, TODAY, DayCounter.actual365_fixed())
    return settings, process


def test_latest_only_matches_explicit_american_and_default_basis():
    """Preserve the default basis and latest-only American value."""
    settings, process = _market()
    latest = VanillaOption.american_until(OptionType.Put, 36.0, EXPIRY, settings)
    explicit = VanillaOption.american(OptionType.Put, 36.0, TODAY, EXPIRY, settings)
    value = latest.price_mc_american(_engine(process))
    default = MCAmericanEngine(
        process,
        steps=25,
        samples=2048,
        seed=42,
        antithetic=True,
        polynomial_order=3,
        calibration_samples=2048,
    )
    assert value == explicit.price_mc_american(default)
    assert latest.exercise_probability() == explicit.exercise_probability()


@pytest.mark.parametrize("basis", [1, 2, 3, 6])
def test_basis_selection_reaches_regression(basis):
    """Exercise each supported weighted regression family."""
    settings, process = _market()
    option = VanillaOption.american_until(OptionType.Put, 36.0, EXPIRY, settings)
    baseline = option.price_mc_american(_engine(process))
    selected = option.price_mc_american(_engine(process, basis))
    assert math.isfinite(selected)
    assert selected != baseline
    assert selected == option.price_mc_american(_engine(process, basis))


@pytest.mark.parametrize("basis", [-1, 4, 5, 7])
def test_bad_basis_recovers(basis):
    """Reject unsupported or unknown bases without affecting valid pricing."""
    settings, process = _market()
    with pytest.raises((ItofinError, ValueError)):
        _engine(process, basis)
    option = VanillaOption.american_until(OptionType.Put, 36.0, EXPIRY, settings)
    assert math.isfinite(option.price_mc_american(_engine(process, 1)))


def test_bermudan_live_rate_retention_and_recovery():
    """Retain schedules and market inputs after Python wrappers are collected."""
    settings, _ = _market()
    dc = DayCounter.actual365_fixed()
    quote = SimpleQuote(0.06)
    rate = FlatForward.from_quote(TODAY, quote, dc)
    dividend = FlatForward(TODAY, 0.0, dc)
    vol = BlackConstantVol(TODAY, 0.2, dc)
    process = BlackScholesProcess.from_curves(36.0, rate, dividend, vol)
    dates = [TODAY + 180, EXPIRY]
    exercise = BermudanExercise(dates)
    option = VanillaOption.from_bermudan(OptionType.Put, 36.0, exercise, settings)
    option.set_mc_american_engine(_engine(process, 2))
    dates.clear()
    del process, exercise, rate, dividend, vol, dc, settings
    gc.collect()
    initial = option.npv()
    quote.set_value(0.08)
    updated = option.npv()
    fresh_settings, fresh_process = _market(0.08)
    fresh = VanillaOption.from_bermudan(
        OptionType.Put,
        36.0,
        BermudanExercise([TODAY + 180, EXPIRY]),
        fresh_settings,
    )
    assert math.isfinite(updated) and updated != initial
    assert updated == fresh.price_mc_american(_engine(fresh_process, 2))
    quote.set_value(0.06)
    del quote
    gc.collect()
    assert option.npv() == initial


@pytest.mark.parametrize("steps", [3, 75])
def test_bermudan_contract_dates_match_independent_quantlib_oracles(steps):
    """Check date-only exercise against independent MC and FD references."""
    import json
    from pathlib import Path

    fixture = Path(__file__).resolve().parents[3] / "sdk/go/testdata/mc_american_depth.json"
    reference = json.loads(fixture.read_text())["bermudan"]
    settings, process = _market()
    settings.set_evaluation_date(TODAY - 2)
    exercise = BermudanExercise([TODAY + days for days in reference["dates"]])
    option = VanillaOption.from_bermudan(OptionType.Put, 40.0, exercise, settings)
    mc = MCAmericanEngine(
        process,
        steps=steps,
        samples=32768,
        seed=42,
        antithetic=True,
        polynomial_order=2,
        calibration_samples=8192,
    )
    value = option.price_mc_american(mc)
    expected = reference["exercise_only_mc"] if steps == 3 else reference["fd"]
    assert math.isfinite(value)
    error = option.error_estimate()
    assert math.isfinite(error) and error > 0.0
    assert abs(value - expected) < 2.34 * error


def test_supported_families_match_independent_quantlib_prices():
    """Check every supported basis against independently seeded QL prices."""
    import json
    from pathlib import Path

    fixture = Path(__file__).resolve().parents[3] / "sdk/go/testdata/mc_american_depth.json"
    settings, process = _market()
    settings.set_evaluation_date(TODAY - 2)
    option = VanillaOption.american_until(OptionType.Put, 40.0, EXPIRY, settings)
    values = []
    for row in json.loads(fixture.read_text())["bases"]:
        mc = MCAmericanEngine(
            process,
            steps=12,
            samples=4096,
            seed=42,
            antithetic=True,
            polynomial_order=2,
            calibration_samples=2048,
            basis_system=row["family"],
        )
        value = option.price_mc_american(mc)
        assert math.isfinite(value)
        error = option.error_estimate()
        assert math.isfinite(error) and error > 0.0
        assert abs(value - row["value"]) < 2.34 * error
        values.append(value)
    assert len(set(values)) == 5


def test_chebyshev_call_domain_error_recovers_with_supported_basis():
    """Reject unbounded call states outside the weighted basis domain."""
    settings, process = _market()
    option = VanillaOption.american_until(OptionType.Call, 30.0, EXPIRY, settings)
    with pytest.raises(ItofinError, match="undefined for in-the-money call"):
        option.price_mc_american(_engine(process, 6))
    assert math.isfinite(option.price_mc_american(_engine(process, 1)))
