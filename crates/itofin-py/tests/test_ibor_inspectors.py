"""The IborIndex inspectors, DayCounter.year_fraction and Date - Date.

Every assertion reconstructs its expectation from the inspectors themselves or
from the values the index was built with, so a stub returning a plausible
constant fails. The two maturity fixtures are picked so that the convention and
the end-of-month flag each change the answer; the divergence guards keep those
reconstructions from going inert if the core roll ever changes.
"""

# pypi/conda library
import pytest

# itofin library
from itofin import Settings
from itofin.indexes import Currency, Euribor, IborIndex
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Period

EURIBOR_FIXING_DAYS = 2
"""The settlement days the core Euribor ctor hard-codes (euribor.rs:56)."""


@pytest.fixture
def settings():
    settings = Settings()
    settings.set_evaluation_date(Date(1, 1, 2020))
    return settings


@pytest.fixture
def euribor6m(settings):
    return Euribor(Period(6, "Months"), None, settings)


def test_generic_index_reports_back_every_convention_it_was_built_with(settings):
    tenor = Period(3, "Months")
    calendar = Calendar.weekends_only()
    day_counter = DayCounter.actual365_fixed()
    index = IborIndex(
        "Probe",
        tenor,
        3,
        Currency.usd(),
        calendar,
        BusinessDayConvention.Preceding,
        False,
        day_counter,
        None,
        settings,
    )

    assert index.tenor() == tenor
    assert index.day_counter() == day_counter
    assert repr(index.fixing_calendar()) == repr(calendar)
    assert index.business_day_convention() == BusinessDayConvention.Preceding
    assert index.end_of_month() is False


def test_euribor_conventions_follow_the_tenor_unit(settings, euribor6m):
    euribor1w = Euribor(Period(1, "Weeks"), None, settings)

    assert euribor6m.business_day_convention() == BusinessDayConvention.ModifiedFollowing
    assert euribor6m.end_of_month() is True
    assert euribor1w.business_day_convention() == BusinessDayConvention.Following
    assert euribor1w.end_of_month() is False

    assert euribor6m.tenor() == Period(6, "Months")
    assert euribor6m.day_counter() == DayCounter.actual360()
    assert repr(euribor6m.fixing_calendar()) == repr(Calendar.target())


def test_value_date_advances_on_the_index_own_fixing_calendar(euribor6m):
    fixing = Date(27, 8, 2019)
    expected = euribor6m.fixing_calendar().advance(
        fixing,
        EURIBOR_FIXING_DAYS,
        "Days",
        BusinessDayConvention.Following,
        False,
    )

    assert euribor6m.value_date(fixing) == expected
    assert euribor6m.fixing_date(expected) == fixing


def test_maturity_date_rolls_under_the_index_own_convention(euribor6m):
    value_date = euribor6m.value_date(Date(27, 8, 2019))
    calendar = euribor6m.fixing_calendar()
    expected = calendar.advance(
        value_date,
        6,
        "Months",
        euribor6m.business_day_convention(),
        euribor6m.end_of_month(),
    )

    assert euribor6m.maturity_date(value_date) == expected
    assert expected != calendar.advance(
        value_date, 6, "Months", BusinessDayConvention.Following, True
    )


def test_maturity_date_rolls_under_the_index_own_end_of_month_flag(euribor6m):
    value_date = euribor6m.value_date(Date(26, 2, 2019))
    calendar = euribor6m.fixing_calendar()
    expected = calendar.advance(
        value_date,
        6,
        "Months",
        euribor6m.business_day_convention(),
        euribor6m.end_of_month(),
    )

    assert euribor6m.maturity_date(value_date) == expected
    assert expected != calendar.advance(
        value_date, 6, "Months", BusinessDayConvention.ModifiedFollowing, False
    )


def test_value_date_rejects_a_non_business_fixing_date(euribor6m):
    with pytest.raises(Exception, match="not a valid fixing date"):
        euribor6m.value_date(Date(25, 12, 2019))


def test_date_minus_date_is_the_signed_day_count():
    assert Date(1, 2, 2020) - Date(1, 1, 2020) == 31
    assert Date(1, 1, 2020) - Date(1, 2, 2020) == -31
    assert Date(1, 1, 2020) - Date(1, 1, 2020) == 0


def test_date_minus_int_still_shifts_the_date():
    assert Date(1, 2, 2020) - 31 == Date(1, 1, 2020)
    assert isinstance(Date(1, 2, 2020) - 31, Date)


def test_date_minus_an_unsupported_type_is_a_type_error():
    with pytest.raises(TypeError):
        Date(1, 2, 2020) - "a week"


def test_year_fraction_divides_the_counter_own_day_count():
    d1, d2 = Date(31, 1, 2019), Date(31, 3, 2019)

    assert DayCounter.actual360().year_fraction(d1, d2) == (d2 - d1) / 360.0
    assert DayCounter.actual365_fixed().year_fraction(d1, d2) == (d2 - d1) / 365.0
    assert DayCounter.thirty360_bond_basis().year_fraction(d1, d2) == 60.0 / 360.0


def test_business_day_convention_variants_keep_their_integer_values():
    assert BusinessDayConvention.ModifiedFollowing == 0
    assert BusinessDayConvention.Following == 1
    assert BusinessDayConvention.Unadjusted == 2
    assert BusinessDayConvention.Preceding == 3
    assert BusinessDayConvention.ModifiedPreceding == 4
    assert BusinessDayConvention.HalfMonthModifiedFollowing == 5
    assert BusinessDayConvention.Nearest == 6
