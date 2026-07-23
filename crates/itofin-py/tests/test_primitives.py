import pytest

from itofin import ItofinError, Settings
from itofin.time import Calendar, Date, DayCounter


def test_date_constructs_and_accessors_round_trip():
    d = Date(15, 6, 2026)
    assert d.year == 2026
    assert d.month == 6
    assert d.day == 15


@pytest.mark.parametrize(
    "day, month, year",
    [
        (15, 13, 2026),
        (0, 1, 2026),
        (31, 2, 2026),
        (1, 1, 1900),
        (1, 1, 2200),
        (29, 2, 2026),
    ],
)
def test_invalid_date_raises_instead_of_panicking(day, month, year):
    with pytest.raises(ItofinError):
        Date(day, month, year)


def test_leap_day_constructs_in_leap_year():
    d = Date(29, 2, 2024)
    assert d.day == 29
    assert d.month == 2
    assert d.year == 2024


def test_date_addition_stays_in_range():
    assert Date(15, 6, 2026) + 90 == Date(13, 9, 2026)


def test_date_subtraction():
    assert Date(13, 9, 2026) - 90 == Date(15, 6, 2026)


def test_date_arithmetic_out_of_range_raises():
    with pytest.raises(ItofinError):
        Date(31, 12, 2199) + 1


def test_settings_constructs_and_is_reusable():
    settings = Settings()
    settings.set_evaluation_date(Date(15, 6, 2026))
    settings.set_evaluation_date(Date(16, 6, 2026))


def test_daycounter_factories_construct():
    assert DayCounter.actual360() is not None
    assert DayCounter.actual365_fixed() is not None


def test_calendar_factories_construct():
    assert Calendar.target() is not None
    assert Calendar.null_calendar() is not None
