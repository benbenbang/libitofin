"""The calendar queries on the Calendar facade.

Every expected value below is pinned against the holiday rules in the core
(`crates/libitofin/src/time/calendars/*.rs`), which mirror QuantLib's
`ql/time/calendars/`. The discriminating date is one where the calendars
disagree: 1 May 2025 (Labour Day) is a holiday on TARGET and a business day
in the UK, whose early-May bank holiday falls on Monday the 5th.

A joint-calendar rule resolves ignoring case; an unknown rule is an
ItofinError before the core is reached, as is an empty joint-calendar list,
which the core asserts on.
"""

# third-party
import pytest

# itofin library
from itofin import ItofinError
from itofin.time import Calendar, Date

LABOUR_DAY_2025 = Date(1, 5, 2025)  # a Thursday
SATURDAY = Date(5, 7, 2025)


def test_a_saturday_is_a_weekend_and_a_holiday_but_not_on_the_null_calendar():
    assert Calendar.target().is_weekend(SATURDAY)
    assert Calendar.target().is_holiday(SATURDAY)
    assert Calendar.weekends_only().is_holiday(SATURDAY)
    assert not Calendar.target().is_weekend(LABOUR_DAY_2025)
    assert Calendar.null_calendar().is_business_day(SATURDAY)


def test_joint_calendar_follows_its_rule():
    uk = Calendar.united_kingdom()
    target = Calendar.target()
    assert uk.is_business_day(LABOUR_DAY_2025)
    assert target.is_holiday(LABOUR_DAY_2025)
    assert Calendar.joint([uk, target]).is_holiday(LABOUR_DAY_2025)
    assert Calendar.joint([uk, target], "JoinHolidays").is_holiday(LABOUR_DAY_2025)
    assert Calendar.joint([uk, target], "JoinBusinessDays").is_business_day(LABOUR_DAY_2025)
    assert Calendar.joint([uk, target], "joinbusinessdays").is_business_day(LABOUR_DAY_2025)
    assert Calendar.joint([uk, target]).name == "JoinHolidays(UK settlement, TARGET)"


def test_joint_calendar_rejects_an_empty_list_and_an_unknown_rule():
    with pytest.raises(ItofinError):
        Calendar.joint([])
    with pytest.raises(ItofinError, match="JoinHolidays, JoinBusinessDays"):
        Calendar.joint([Calendar.target()], "Union")


def test_name_reflects_the_calendar():
    assert Calendar.target().name == "TARGET"
    assert Calendar.united_kingdom().name == "UK settlement"
    assert repr(Calendar.target()) == "Calendar(TARGET)"


def test_equality_and_hash_are_by_name():
    assert Calendar.target() == Calendar.target()
    assert hash(Calendar.target()) == hash(Calendar.target())
    assert Calendar.target() != Calendar.united_kingdom()
    assert len({Calendar.target(), Calendar.target(), Calendar.united_kingdom()}) == 2
    assert {Calendar.united_kingdom(): "gbp"}[Calendar.united_kingdom()] == "gbp"


def test_holiday_list_over_december_on_target():
    holidays = Calendar.target().holiday_list(Date(1, 12, 2025), Date(31, 12, 2025))
    assert holidays == [Date(25, 12, 2025), Date(26, 12, 2025)]
    with_weekends = Calendar.target().holiday_list(
        Date(1, 12, 2025), Date(31, 12, 2025), include_weekends=True
    )
    assert len(with_weekends) == 2 + 8
    assert Date(6, 12, 2025) in with_weekends


def test_business_days_between_on_target_around_christmas():
    target = Calendar.target()
    monday, next_monday = Date(22, 12, 2025), Date(29, 12, 2025)
    assert target.business_days_between(monday, next_monday) == 3
    assert target.business_days_between(monday, next_monday, include_last=True) == 4
    assert target.business_days_between(monday, next_monday, include_first=False) == 2
    assert target.business_days_between(monday, monday) == 0


def test_holiday_list_rejects_a_reversed_range():
    with pytest.raises(ItofinError):
        Calendar.target().holiday_list(Date(31, 12, 2025), Date(1, 12, 2025))
