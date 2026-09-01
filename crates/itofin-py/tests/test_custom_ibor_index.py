"""The general three-calendar CustomIborIndex facade (#964).

The date table of custom.rs:5-13 has one pin per calendar role, and the fixture
assigns three deliberately DIVERGING calendars so each pin moves under exactly
one role: fixing on the UK, value on TARGET, maturity on WeekendsOnly. A
fixture that reused one calendar in two roles would leave a role-collapse
mutant invisible, the vacuity #963 was rewritten to avoid.

Each expected date below is stated with the answer the role-collapse mutant
gives, and the mutants were run: collapsing any one role changes exactly the
pin named for it.
"""

# itofin library
from itofin import Settings
from itofin.indexes import Currency, CustomIborIndex, IborIndex
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Period

TODAY = Date(15, 6, 2021)

# Wednesday 1 September 2021 back two TARGET business days is Monday the 30th,
# the UK Summer Bank Holiday; the Preceding adjust on the UK fixing calendar
# then falls to Friday the 27th. A fixing calendar collapsed to TARGET stops at
# the 30th.
FIXING_ROLL_VALUE_DATE = Date(1, 9, 2021)
EXPECTED_FIXING_DATE = Date(27, 8, 2021)

# Thursday 26 August 2021 plus two TARGET business days is Monday the 30th,
# which WeekendsOnly leaves alone. A value calendar collapsed to the UK one
# skips the Summer Bank Holiday and answers Tuesday the 31st.
VALUE_ROLL_FIXING_DATE = Date(26, 8, 2021)
EXPECTED_VALUE_DATE = Date(30, 8, 2021)

# Monday 4 January 2021 plus 3M is Easter Sunday, 4 April; WeekendsOnly rolls
# it to Monday the 5th. A maturity calendar collapsed to either the UK or
# TARGET calendar hits Easter Monday and answers Tuesday the 6th.
MATURITY_ROLL_VALUE_DATE = Date(4, 1, 2021)
EXPECTED_MATURITY_DATE = Date(5, 4, 2021)


def _three_calendar_index(fixing, value, maturity):
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    return CustomIborIndex(
        "Custom",
        Period(3, "Months"),
        2,
        Currency.eur(),
        fixing,
        value,
        maturity,
        BusinessDayConvention.ModifiedFollowing,
        False,
        DayCounter.actual360(),
        None,
        settings,
    )


def test_each_date_role_rolls_on_its_own_calendar():
    """The custom.rs:5-13 date table over three diverging calendars: one pin per
    role, each stated against the date its role-collapse mutant returns."""
    index = _three_calendar_index(
        Calendar.united_kingdom(), Calendar.target(), Calendar.weekends_only()
    )

    assert isinstance(index, IborIndex)
    assert index.name() == "Custom3M Actual/360"
    assert index.fixing_date(FIXING_ROLL_VALUE_DATE) == EXPECTED_FIXING_DATE
    assert index.value_date(VALUE_ROLL_FIXING_DATE) == EXPECTED_VALUE_DATE
    assert index.maturity_date(MATURITY_ROLL_VALUE_DATE) == EXPECTED_MATURITY_DATE


def test_one_calendar_in_all_three_roles_matches_a_plain_index():
    """The degenerate assignment is the plain single-calendar index: passing the
    UK calendar three times reproduces IborIndex on that calendar, so the three
    pins above are the divergence and not an artefact of the facade."""
    index = _three_calendar_index(
        Calendar.united_kingdom(), Calendar.united_kingdom(), Calendar.united_kingdom()
    )
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    plain = IborIndex(
        "Custom",
        Period(3, "Months"),
        2,
        Currency.eur(),
        Calendar.united_kingdom(),
        BusinessDayConvention.ModifiedFollowing,
        False,
        DayCounter.actual360(),
        None,
        settings,
    )

    assert index.fixing_date(FIXING_ROLL_VALUE_DATE) == plain.fixing_date(
        FIXING_ROLL_VALUE_DATE
    )
    assert index.value_date(VALUE_ROLL_FIXING_DATE) == plain.value_date(
        VALUE_ROLL_FIXING_DATE
    )
    assert index.maturity_date(MATURITY_ROLL_VALUE_DATE) == plain.maturity_date(
        MATURITY_ROLL_VALUE_DATE
    )
    assert index.value_date(VALUE_ROLL_FIXING_DATE) != EXPECTED_VALUE_DATE
