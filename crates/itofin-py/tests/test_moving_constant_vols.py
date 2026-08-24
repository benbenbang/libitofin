"""Moving (floating-reference) constructors on the two constant vol facades (#627).

The moving forms derive the reference date from the Settings evaluation date -
settlement days on a calendar - instead of pinning it at construction, the
shape live market data wants. Two arms per facade:

A. COINCIDENCE: the moving surface at settlement 0 has the same reference date
   as the fixed-reference surface built AT the evaluation date, and answers the
   same volatility. On a constant surface the volatility agreement alone is
   vacuous (it is the same number everywhere by construction), so the
   reference-date equality is the load-bearing half of this arm; the volatility
   assert just pins that the level went through.

B. FOLLOW: settings.set_evaluation_date(later) moves the moving surface's
   reference date to the new date while the fixed surface's stays where it was
   built. This is the property the moving forms exist for, and the reason the
   base classes now expose reference_date().

The quote-backed forms additionally pin liveness: a set_value on the caller's
quote changes what the moving surface answers, which the internal-quote form
cannot do.

Each test builds its own Settings so no arm's evaluation-date move can leak
into another (the two-Settings lesson of test_capfloor.py).
"""

from itofin import Settings
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    ConstantOptionletVolatility,
    ConstantSwaptionVolatility,
    VolatilityType,
)
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Period

EVAL = Date(15, 1, 2026)
LATER = Date(16, 2, 2026)
VOL = 0.20
BUMPED = 0.25

FOLLOWING = BusinessDayConvention.Following
LOGNORMAL = VolatilityType.ShiftedLognormal

ONE_YEAR = Period(1, "Years")
FIVE_YEARS = Period(5, "Years")
STRIKE = 0.03


def _settings():
    settings = Settings()
    settings.set_evaluation_date(EVAL)
    return settings


def _swaption_fixed(reference_date):
    return ConstantSwaptionVolatility(
        reference_date,
        Calendar.target(),
        FOLLOWING,
        VOL,
        DayCounter.actual365_fixed(),
        LOGNORMAL,
    )


def _swaption_moving(settings):
    return ConstantSwaptionVolatility.moving(
        0,
        Calendar.target(),
        FOLLOWING,
        VOL,
        DayCounter.actual365_fixed(),
        LOGNORMAL,
        settings,
    )


def test_the_moving_swaption_surface_coincides_with_the_fixed_one_at_eval():
    settings = _settings()
    fixed = _swaption_fixed(EVAL)
    moving = _swaption_moving(settings)

    assert moving.reference_date() == fixed.reference_date()
    assert moving.reference_date() == EVAL
    fixed_vol = fixed.volatility(ONE_YEAR, FIVE_YEARS, STRIKE)
    moving_vol = moving.volatility(ONE_YEAR, FIVE_YEARS, STRIKE)
    assert abs(moving_vol - fixed_vol) < 1.0e-12
    assert abs(moving_vol - VOL) < 1.0e-12


def test_the_moving_swaption_surface_follows_the_evaluation_date():
    settings = _settings()
    fixed = _swaption_fixed(EVAL)
    moving = _swaption_moving(settings)
    assert moving.reference_date() == EVAL

    settings.set_evaluation_date(LATER)

    assert moving.reference_date() == LATER
    assert fixed.reference_date() == EVAL


def test_the_quote_backed_moving_swaption_surface_tracks_its_quote():
    settings = _settings()
    quote = SimpleQuote(VOL)
    moving = ConstantSwaptionVolatility.moving_with_quote(
        0,
        Calendar.target(),
        FOLLOWING,
        quote,
        DayCounter.actual365_fixed(),
        LOGNORMAL,
        settings,
    )
    assert moving.reference_date() == EVAL
    assert abs(moving.volatility(ONE_YEAR, FIVE_YEARS, STRIKE) - VOL) < 1.0e-12

    quote.set_value(BUMPED)
    assert abs(moving.volatility(ONE_YEAR, FIVE_YEARS, STRIKE) - BUMPED) < 1.0e-12


def _optionlet_fixed(reference_date):
    return ConstantOptionletVolatility(
        reference_date,
        Calendar.target(),
        FOLLOWING,
        VOL,
        DayCounter.actual365_fixed(),
        LOGNORMAL,
    )


def _optionlet_moving(settings):
    return ConstantOptionletVolatility.moving(
        0,
        Calendar.target(),
        FOLLOWING,
        VOL,
        DayCounter.actual365_fixed(),
        LOGNORMAL,
        settings,
    )


def test_the_moving_optionlet_surface_coincides_with_the_fixed_one_at_eval():
    settings = _settings()
    fixed = _optionlet_fixed(EVAL)
    moving = _optionlet_moving(settings)

    assert moving.reference_date() == fixed.reference_date()
    assert moving.reference_date() == EVAL
    fixed_vol = fixed.volatility(ONE_YEAR, STRIKE)
    moving_vol = moving.volatility(ONE_YEAR, STRIKE)
    assert abs(moving_vol - fixed_vol) < 1.0e-12
    assert abs(moving_vol - VOL) < 1.0e-12


def test_the_moving_optionlet_surface_follows_the_evaluation_date():
    settings = _settings()
    fixed = _optionlet_fixed(EVAL)
    moving = _optionlet_moving(settings)
    assert moving.reference_date() == EVAL

    settings.set_evaluation_date(LATER)

    assert moving.reference_date() == LATER
    assert fixed.reference_date() == EVAL


def test_the_quote_backed_moving_optionlet_surface_tracks_its_quote():
    settings = _settings()
    quote = SimpleQuote(VOL)
    moving = ConstantOptionletVolatility.moving_with_quote(
        0,
        Calendar.target(),
        FOLLOWING,
        quote,
        DayCounter.actual365_fixed(),
        LOGNORMAL,
        settings,
    )
    assert moving.reference_date() == EVAL
    assert abs(moving.volatility(ONE_YEAR, STRIKE) - VOL) < 1.0e-12

    quote.set_value(BUMPED)
    assert abs(moving.volatility(ONE_YEAR, STRIKE) - BUMPED) < 1.0e-12
