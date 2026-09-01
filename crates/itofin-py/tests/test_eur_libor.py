"""The EurLibor facade: a three-calendar index passed into a FRA (#964).

EurLibor is the only Py index family whose core constructor returns a
CustomIborIndex rather than a plain IborIndex, so the facade reaches the base
half through upcast() (custom.rs:105). What that has to carry is the
three-calendar roll: fixing dates on the joint UK-Exchange-plus-TARGET
calendar, value and maturity dates on TARGET alone (eurlibor.rs:64).

The mutant every date pin here is aimed at is a facade that quietly built a
single-calendar IborIndex on the joint fixing calendar - the shape the other
four families legitimately use. That mutant is not hypothetical prose: it is
constructed in-test as `_single_calendar_twin`, off the EurLibor's own
inspectors so the fixing calendar is structurally the same one, and every
literal below is stated alongside the twin's differing answer.

The date fixtures are the merged #939 core oracles (eurlibor.rs:216, :232),
which sit on UK bank holidays TARGET stays open for. The FRA numbers are
rebuilt from math.exp and Actual360 day counts over DISTINCT 4%/6% curves, the
same independent oracle test_fra.py uses.
"""

# standard library
import datetime
import math

# pypi/conda library
import pytest

# itofin library
from itofin import Settings
from itofin.indexes import EurLibor, IborIndex
from itofin.instruments import ForwardRateAgreement, Position
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Period

TODAY = Date(15, 6, 2021)
FORWARD_FLAT = 0.04
DISCOUNT_FLAT = 0.06
STRIKE = 0.02
NOTIONAL = 100.0

# Thursday 26 August 2021 is a business day on every calendar in play. Two
# TARGET business days on is Monday the 30th, the UK Summer Bank Holiday that
# TARGET stays open for; the joint calendar skips it to Tuesday the 31st.
FIXING_DATE = Date(26, 8, 2021)
TARGET_VALUE_DATE = Date(30, 8, 2021)
JOINT_VALUE_DATE = Date(31, 8, 2021)
FRA_MATURITY_DATE = Date(30, 11, 2021)

# Monday 1 February 2021 plus 3M is Saturday 1 May; ModifiedFollowing rolls to
# Monday the 3rd on TARGET (whose May holiday is the 1st) but on to Tuesday the
# 4th on the joint calendar, where the 3rd is the UK Early May Bank Holiday.
MATURITY_ROLL_START = Date(1, 2, 2021)
TARGET_MATURITY_DATE = Date(3, 5, 2021)
JOINT_MATURITY_DATE = Date(4, 5, 2021)


def _actual360(start: Date, end: Date) -> float:
    days = (
        datetime.date(end.year, end.month, end.day)
        - datetime.date(start.year, start.month, start.day)
    ).days
    return days / 360.0


def _discount(rate: float, date: Date) -> float:
    return math.exp(-rate * _actual360(TODAY, date))


def _expected_forward(value_date: Date, maturity_date: Date) -> float:
    return (
        _discount(FORWARD_FLAT, value_date) / _discount(FORWARD_FLAT, maturity_date)
        - 1.0
    ) / _actual360(value_date, maturity_date)


@pytest.fixture
def settings():
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    return settings


@pytest.fixture
def forwarding():
    return FlatForward(TODAY, FORWARD_FLAT, DayCounter.actual360())


@pytest.fixture
def discounting():
    return FlatForward(TODAY, DISCOUNT_FLAT, DayCounter.actual360())


@pytest.fixture
def eur_libor(settings, forwarding):
    return EurLibor(Period(3, "Months"), forwarding, settings)


def _single_calendar_twin(eur_libor, settings, forwarding):
    """The mutant the date pins exist to kill: the same conventions on the same
    fixing calendar, but that one calendar in all three date roles."""
    return IborIndex(
        "EURLibor",
        eur_libor.tenor(),
        2,
        eur_libor.currency(),
        eur_libor.fixing_calendar(),
        eur_libor.business_day_convention(),
        eur_libor.end_of_month(),
        eur_libor.day_counter(),
        forwarding,
        settings,
    )


def test_eur_libor_carries_the_ice_configuration(eur_libor):
    """The eurlibor.cpp:60-78 configuration. The name pin kills a facade wired
    to Euribor (the other EUR family) and the calendar pin kills one whose
    fixing calendar is bare TARGET: Monday 30 August 2021 is a TARGET business
    day, so only the joint calendar moves it on to the 31st."""
    assert isinstance(eur_libor, IborIndex)
    assert eur_libor.name() == "EURLibor3M Actual/360"
    assert eur_libor.tenor() == Period(3, "Months")
    assert eur_libor.currency().code() == "EUR"
    assert eur_libor.business_day_convention() == BusinessDayConvention.ModifiedFollowing
    assert eur_libor.end_of_month()

    joint = eur_libor.fixing_calendar()
    assert joint.adjust(TARGET_VALUE_DATE, BusinessDayConvention.Following) == (
        JOINT_VALUE_DATE
    )
    assert Calendar.target().adjust(
        TARGET_VALUE_DATE, BusinessDayConvention.Following
    ) == TARGET_VALUE_DATE


def test_base_half_rolls_value_and_maturity_on_target(eur_libor, settings, forwarding):
    """The upcast proof, read through the base IborIndex half every consumer
    holds: both dates roll on TARGET, not on the joint fixing calendar. The
    single-calendar twin answers 31 August and 4 May on the same two inputs, so
    each literal is one calendar role's discrimination."""
    twin = _single_calendar_twin(eur_libor, settings, forwarding)

    assert eur_libor.value_date(FIXING_DATE) == TARGET_VALUE_DATE
    assert twin.value_date(FIXING_DATE) == JOINT_VALUE_DATE

    assert eur_libor.maturity_date(MATURITY_ROLL_START) == TARGET_MATURITY_DATE
    assert twin.maturity_date(MATURITY_ROLL_START) == JOINT_MATURITY_DATE


def test_daily_tenor_is_rejected(settings, forwarding):
    """The daily-tenor guard (eurlibor.rs:87) surfaces as an ItofinError; the
    dedicated DailyTenorEURLibor constructor is not ported."""
    with pytest.raises(Exception, match="DailyTenor"):
        EurLibor(Period(3, "Days"), forwarding, settings)


def test_fra_over_eur_libor_prices_off_the_target_window(eur_libor, discounting):
    """Pass-into-instrument: a Python EurLibor drives a ForwardRateAgreement.
    The window opens on the index's own value date, so it is the TARGET one -
    30 August, the day the single-calendar twin skips. The rate, amount and NPV
    are the hand-built par formulas over that window off the 4% forwarding
    curve, discounted on the 6% curve."""
    value_date = eur_libor.value_date(FIXING_DATE)
    fra = ForwardRateAgreement(
        eur_libor, value_date, Position.Long, STRIKE, NOTIONAL, discounting
    )

    assert fra.value_date() == TARGET_VALUE_DATE
    assert fra.maturity_date() == FRA_MATURITY_DATE

    expected_forward = _expected_forward(TARGET_VALUE_DATE, FRA_MATURITY_DATE)
    assert abs(expected_forward - STRIKE) > 0.01, (
        "degenerate fixture: K == F would leave the amount pin vacuous"
    )
    assert abs(fra.forward_rate() - expected_forward) < 1.0e-12

    accrual = _actual360(TARGET_VALUE_DATE, FRA_MATURITY_DATE)
    expected_amount = (
        NOTIONAL * (expected_forward - STRIKE) * accrual / (1.0 + expected_forward * accrual)
    )
    assert abs(fra.amount() - expected_amount) < 1.0e-12

    expected_npv = expected_amount * _discount(DISCOUNT_FLAT, TARGET_VALUE_DATE)
    assert abs(fra.npv() - expected_npv) < 1.0e-12

    assert abs(eur_libor.fixing(FIXING_DATE, False) - expected_forward) < 1.0e-12


def test_fra_window_differs_from_a_single_calendar_index(
    eur_libor, settings, forwarding, discounting
):
    """The discrimination arm for the instrument leg. Both FRAs start from the
    same fixing date and let the index set the window; the EurLibor one opens
    on 30 August and the single-calendar twin on the 31st, and the day of
    accrual that separates them is worth ~5.6e-3 of NPV on a notional of 100 -
    a facade that had lost the three-calendar roll would price the twin's
    number here."""
    twin = _single_calendar_twin(eur_libor, settings, forwarding)

    fra = ForwardRateAgreement(
        eur_libor,
        eur_libor.value_date(FIXING_DATE),
        Position.Long,
        STRIKE,
        NOTIONAL,
        discounting,
    )
    twin_fra = ForwardRateAgreement(
        twin, twin.value_date(FIXING_DATE), Position.Long, STRIKE, NOTIONAL, discounting
    )

    assert fra.value_date() == TARGET_VALUE_DATE
    assert twin_fra.value_date() == JOINT_VALUE_DATE
    assert abs(fra.npv() - twin_fra.npv()) > 1.0e-6
