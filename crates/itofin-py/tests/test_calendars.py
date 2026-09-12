"""The named national calendars and the calendar queries on the Calendar facade.

Every expected value below is pinned against the holiday rules in the core
(`crates/libitofin/src/time/calendars/*.rs`), which mirror QuantLib's
`ql/time/calendars/`. The discriminating dates are the ones where calendars
disagree: 4 July 2025 is a holiday on both US markets and a business day on
TARGET and in the UK; 25 December 2025 is a holiday across Europe but a
business day in Japan, whose December holiday is the 31st (bank holiday) and,
before 2019, the Emperor's birthday on the 23rd.

Market names resolve ignoring case, so "NYSE", "Nyse" and "nyse" build the
same calendar; an unknown market or joint rule is an ItofinError before the
core is reached, as is an empty joint-calendar list, which the core asserts on.
"""

# third-party
import pytest

# itofin library
from itofin import ItofinError
from itofin.time import BusinessDayConvention, Calendar, Date

INDEPENDENCE_DAY_2025 = Date(4, 7, 2025)  # a Friday
JUNETEENTH_2025 = Date(19, 6, 2025)  # a Thursday
CHRISTMAS_2025 = Date(25, 12, 2025)  # a Thursday
NEW_YEAR_2025 = Date(1, 1, 2025)  # a Wednesday
SATURDAY = Date(5, 7, 2025)


def test_independence_day_is_a_us_holiday_but_a_target_business_day():
    assert Calendar.united_states("NYSE").is_holiday(INDEPENDENCE_DAY_2025)
    assert Calendar.united_states("Settlement").is_holiday(INDEPENDENCE_DAY_2025)
    assert Calendar.target().is_business_day(INDEPENDENCE_DAY_2025)
    assert Calendar.united_kingdom().is_business_day(INDEPENDENCE_DAY_2025)


def test_juneteenth_is_a_nyse_holiday():
    assert Calendar.united_states("NYSE").is_holiday(JUNETEENTH_2025)
    assert not Calendar.united_states("NYSE").is_business_day(JUNETEENTH_2025)


def test_christmas_is_a_european_holiday_and_a_japanese_business_day():
    for calendar in (
        Calendar.target(),
        Calendar.united_kingdom(),
        Calendar.germany(),
        Calendar.germany("FrankfurtStockExchange"),
    ):
        assert calendar.is_holiday(CHRISTMAS_2025), calendar
    assert Calendar.japan().is_business_day(CHRISTMAS_2025)
    assert Calendar.japan().is_holiday(Date(31, 12, 2025))


def test_new_year_is_a_holiday_on_every_western_calendar():
    for calendar in (
        Calendar.target(),
        Calendar.united_states(),
        Calendar.united_kingdom(),
        Calendar.germany(),
        Calendar.japan(),
        Calendar.switzerland(),
        Calendar.hong_kong(),
    ):
        assert calendar.is_holiday(NEW_YEAR_2025), calendar


def test_a_saturday_is_a_weekend_and_a_holiday_but_not_on_the_null_calendar():
    assert Calendar.target().is_weekend(SATURDAY)
    assert Calendar.target().is_holiday(SATURDAY)
    assert Calendar.weekends_only().is_holiday(SATURDAY)
    assert not Calendar.target().is_weekend(INDEPENDENCE_DAY_2025)
    assert Calendar.null_calendar().is_business_day(SATURDAY)


def test_joint_calendar_follows_its_rule():
    us = Calendar.united_states()
    uk = Calendar.united_kingdom()
    assert Calendar.joint([us, uk]).is_holiday(INDEPENDENCE_DAY_2025)
    assert Calendar.joint([us, uk], "JoinHolidays").is_holiday(INDEPENDENCE_DAY_2025)
    assert Calendar.joint([us, uk], "JoinBusinessDays").is_business_day(
        INDEPENDENCE_DAY_2025
    )
    assert Calendar.joint([us, uk], "joinbusinessdays").is_business_day(
        INDEPENDENCE_DAY_2025
    )
    assert Calendar.joint([us, uk]).name == "JoinHolidays(US settlement, UK settlement)"


def test_joint_calendar_rejects_an_empty_list_and_an_unknown_rule():
    with pytest.raises(ItofinError):
        Calendar.joint([])
    with pytest.raises(ItofinError, match="JoinHolidays, JoinBusinessDays"):
        Calendar.joint([Calendar.target()], "Union")


def test_unknown_market_raises_listing_the_accepted_names():
    with pytest.raises(ItofinError, match="Settlement, NYSE, GovernmentBond"):
        Calendar.united_states("LSE")
    with pytest.raises(ItofinError, match="Settlement, Exchange, Metals"):
        Calendar.united_kingdom("Nyse")
    with pytest.raises(ItofinError, match="Merval"):
        Calendar.argentina("Buenos Aires")


def test_market_names_resolve_ignoring_case():
    nyse = Calendar.united_states("NYSE")
    assert Calendar.united_states("Nyse") == nyse
    assert Calendar.united_states("nyse") == nyse
    assert Calendar.germany("xetra") == Calendar.germany("Xetra")


def test_name_reflects_the_market():
    assert Calendar.united_states().name == "US settlement"
    assert Calendar.united_states("NYSE").name == "New York stock exchange"
    assert Calendar.united_states("GovernmentBond").name == "US government bond market"
    assert Calendar.target().name == "TARGET"
    assert repr(Calendar.target()) == "Calendar(TARGET)"


def test_equality_and_hash_are_by_name():
    assert Calendar.target() == Calendar.target()
    assert hash(Calendar.target()) == hash(Calendar.target())
    assert Calendar.target() != Calendar.united_kingdom()
    assert Calendar.united_states("NYSE") != Calendar.united_states("Settlement")
    assert len({Calendar.target(), Calendar.target(), Calendar.japan()}) == 2
    assert {Calendar.united_kingdom(): "gbp"}[Calendar.united_kingdom("settlement")] == "gbp"


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


@pytest.mark.parametrize(
    "build",
    [
        Calendar.target,
        Calendar.null_calendar,
        Calendar.weekends_only,
        Calendar.argentina,
        Calendar.australia,
        Calendar.austria,
        Calendar.botswana,
        Calendar.brazil,
        Calendar.canada,
        Calendar.chile,
        Calendar.china,
        Calendar.croatia,
        Calendar.czech_republic,
        Calendar.denmark,
        Calendar.finland,
        Calendar.france,
        Calendar.germany,
        Calendar.hong_kong,
        Calendar.hungary,
        Calendar.iceland,
        Calendar.india,
        Calendar.indonesia,
        Calendar.israel,
        Calendar.italy,
        Calendar.japan,
        Calendar.malta,
        Calendar.mexico,
        Calendar.montenegro,
        Calendar.new_zealand,
        Calendar.north_macedonia,
        Calendar.norway,
        Calendar.poland,
        Calendar.romania,
        Calendar.russia,
        Calendar.saudi_arabia,
        Calendar.serbia,
        Calendar.singapore,
        Calendar.slovakia,
        Calendar.slovenia,
        Calendar.south_africa,
        Calendar.south_korea,
        Calendar.sweden,
        Calendar.switzerland,
        Calendar.taiwan,
        Calendar.thailand,
        Calendar.turkey,
        Calendar.ukraine,
        Calendar.united_kingdom,
        Calendar.united_states,
        Calendar.uzbekistan,
    ],
)
def test_every_constructor_builds_without_arguments(build):
    calendar = build()
    assert calendar.name
    # Indonesia and Saudi Arabia tabulate their lunar holidays only through
    # 2014 and 2022, so the probe stays early. Every calendar but the null one
    # holds at least its weekends over a year, whatever days those fall on.
    holidays = calendar.holiday_list(Date(1, 1, 2010), Date(31, 12, 2010), include_weekends=True)
    assert (len(holidays) > 0) == (calendar != Calendar.null_calendar())
    assert all(calendar.is_holiday(d) for d in holidays)


@pytest.mark.parametrize(
    ("build", "horizon"),
    [(Calendar.indonesia, 2014), (Calendar.saudi_arabia, 2022)],
)
def test_calendars_with_a_holiday_horizon_raise_past_it(build, horizon):
    calendar = build()
    inside, outside = Date(1, 6, horizon), Date(1, 1, horizon + 1)
    assert calendar.is_business_day(inside) in (True, False)
    for query in (calendar.is_business_day, calendar.is_holiday, calendar.is_weekend):
        with pytest.raises(ItofinError, match=f"tabulated only through {horizon}"):
            query(outside)
    with pytest.raises(ItofinError):
        calendar.holiday_list(inside, outside)
    with pytest.raises(ItofinError):
        calendar.business_days_between(inside, outside)
    with pytest.raises(ItofinError):
        calendar.adjust(outside, BusinessDayConvention.Following)
    with pytest.raises(ItofinError):
        calendar.advance(outside, 1, "Days", BusinessDayConvention.Following, False)


def test_holiday_list_rejects_a_reversed_range():
    with pytest.raises(ItofinError):
        Calendar.target().holiday_list(Date(31, 12, 2025), Date(1, 12, 2025))
