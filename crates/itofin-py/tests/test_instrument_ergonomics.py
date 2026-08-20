"""Oracle for the instrument ergonomics surface (#882): calculate(),
is_calculated(), the per-facade one-shot price(arg) and the frozen results()
snapshot.

The surface layers over the core's lazy-object contract rather than replacing
it: set_*_engine still only wires the observer and marks the cache stale, and
the valuation still fires on the first accessor. What is new is that the
firing, the cache state and the copied outputs are all sayable from Python.

This file carries the VanillaOption pass. Two things it pins are worth naming.

A. price(process) is exactly set_engine(process) + npv(), same float, one call.
   The equality is bit-exact, not a tolerance: it is the same code path.

B. The evaluation-date arm pins the OBSERVER contract only, and deliberately
   NOT a price change. A VanillaOption registers with the settings evaluation
   date in its constructor (oneassetoption.rs:159), so moving that date does
   invalidate the cache. But BlackScholesProcess builds FIXED-reference curves
   (market.rs:86-100) and the analytic European engine reads every input off
   them by absolute date - black_variance_date, discount_date, reference_date
   (pricingengines/vanilla/mod.rs:105-133). The evaluation date therefore never
   enters this engine's price, and the recalculation reproduces the old NPV
   BIT FOR BIT. Asserting a moved price here would be asserting something
   false. The genuine frozen-copy discriminator, which needs an input the price
   really depends on, lives on CapFloor in this file's second pass.

   The moved date must also stay well inside the option's life. Past expiry the
   core short-circuits to setup_expired() and reports a zero NPV, and the arm
   would then pass for entirely the wrong reason.

Also pinned honestly rather than hopefully: the analytic engine fills neither
error_estimate nor valuation_date, so both read None on the snapshot. Its seven
extra outputs are all real-valued, so all seven survive the Real-only
additional_results copy, and the two that the fixture states outright - the
strike and the spot - are checked against the numbers the fixture was built
with, so a snapshot that copied the keys but garbled the values fails.
"""

import pytest

from itofin import ItofinError, Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.processes import BlackScholesProcess
from itofin.time import Date, DayCounter

REF = Date(15, 1, 2026)
EXPIRY = Date(15, 1, 2027)
MID_LIFE = Date(15, 6, 2026)

SPOT = 100.0
STRIKE = 100.0
RISK_FREE = 0.03
DIVIDEND = 0.01
VOL = 0.20


def _process():
    return BlackScholesProcess(
        SPOT, RISK_FREE, DIVIDEND, VOL, REF, DayCounter.actual365_fixed()
    )


def _option(settings):
    return VanillaOption(OptionType.Call, STRIKE, EXPIRY, settings)


def _settings():
    settings = Settings()
    settings.set_evaluation_date(REF)
    return settings


def test_price_is_set_engine_plus_npv():
    settings = _settings()
    one_shot = _option(settings)
    long_hand = _option(settings)
    long_hand.set_engine(_process())

    assert one_shot.price(_process()) == long_hand.npv()


def test_an_option_starts_uncalculated_and_latches_once_priced():
    settings = _settings()
    option = _option(settings)
    assert not option.is_calculated()

    option.price(_process())
    assert option.is_calculated()


def test_calculate_leaves_the_npv_reachable_and_is_idempotent():
    settings = _settings()
    option = _option(settings)
    option.set_engine(_process())

    option.calculate()
    priced = option.npv()
    option.calculate()

    assert option.is_calculated()
    assert option.npv() == priced


def test_the_snapshot_reports_the_npv_just_priced():
    settings = _settings()
    option = _option(settings)
    priced = option.price(_process())

    assert option.results().npv == priced


def test_the_analytic_engine_fills_neither_error_estimate_nor_valuation_date():
    """Asserted as the engine actually behaves, not as a richer engine would:
    pricingengines/vanilla/mod.rs sets results.value and the greeks and never
    touches the other two instrument-level fields."""
    settings = _settings()
    option = _option(settings)
    option.price(_process())
    snapshot = option.results()

    assert snapshot.error_estimate is None
    assert snapshot.valuation_date is None


def test_the_snapshot_carries_the_engines_real_valued_tags():
    """The seven tags the analytic engine writes are all reals, so all seven
    survive the Real-only copy. Two of them are inputs the fixture states, so
    they pin the values rather than only the keys."""
    settings = _settings()
    option = _option(settings)
    option.price(_process())
    extras = option.results().additional_results

    assert set(extras) == {
        "spot",
        "strike",
        "forward",
        "volatility",
        "timeToExpiry",
        "riskFreeDiscount",
        "dividendDiscount",
    }
    assert extras["spot"] == SPOT
    assert extras["strike"] == STRIKE
    assert extras["volatility"] == pytest.approx(VOL, abs=1e-12)


def test_an_evaluation_date_move_invalidates_the_cache():
    """The observer half of the lazy contract. See section B of the module
    docstring for why the recalculated price is UNCHANGED here, and why that is
    the correct assertion rather than a weak one."""
    settings = _settings()
    option = _option(settings)
    priced = option.price(_process())
    assert option.is_calculated()

    settings.set_evaluation_date(MID_LIFE)
    assert not option.is_calculated()

    reread = option.npv()
    assert option.is_calculated()
    assert reread == priced


def test_the_snapshot_survives_the_evaluation_date_move():
    settings = _settings()
    option = _option(settings)
    priced = option.price(_process())
    snapshot = option.results()

    settings.set_evaluation_date(MID_LIFE)

    assert snapshot.npv == priced


def test_the_snapshot_is_read_only():
    settings = _settings()
    option = _option(settings)
    option.price(_process())
    snapshot = option.results()

    with pytest.raises(AttributeError):
        snapshot.npv = 0.0


def test_calculating_without_an_engine_raises():
    settings = _settings()
    option = _option(settings)

    with pytest.raises(ItofinError) as raised:
        option.calculate()
    assert "null pricing engine" in str(raised.value)

    with pytest.raises(ItofinError):
        option.results()
