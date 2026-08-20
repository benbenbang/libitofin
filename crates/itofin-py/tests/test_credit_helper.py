"""Structural smoke for the credit-helper facades and the gaps they need (#740).

The numeric oracle for all three is the bootstrap round-trip in A5; what is
pinned here is that each facade is wired to the mechanism it claims, which a
repricing oracle would not discriminate:

* the ``rule`` parameter reaches ``MakeSchedule::with_rule``. A dropped
  parameter still builds a schedule, just a plain quarterly one, so the pin is
  the rule's own signature - every interior date on the twentieth of an IMM
  month - rather than the schedule merely existing.
* the default rule still reproduces what the facade built before it took one.
  ``forwards()`` is ``with_rule(DateGeneration::Forward)`` (schedule.rs:852-853),
  so the two are identical by construction; this pins the default at the Python
  boundary.
* ``SpreadCdsHelper`` builds under every date-generation rule the core takes,
  and the three post-Big-Bang ones reach ``cdsMaturity``: their pillar is the
  hand-derived CDS roll date, which a helper that had dropped the rule would
  miss by a quarter.
* ``Settings.include_todays_cash_flows`` round-trips its three states. Whether
  the flag moves a price is A5's.
"""

# pypi/conda library
import pytest

# itofin library
from itofin import Settings
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward, SpreadCdsHelper
from itofin.time import BusinessDayConvention, Calendar, Date, DateGeneration, DayCounter, Frequency, Period, Schedule

TODAY = Date(15, 5, 2007)
END = Date(15, 5, 2012)
IMM_MONTHS = {3, 6, 9, 12}


def a_quarterly_schedule(rule=None):
    """A five-year quarterly schedule, unadjusted so the rule's own dates show
    through: under Following the twentieth rolls to the 21st or 22nd whenever it
    falls on a weekend, which would blunt the pin."""
    args = (
        TODAY,
        END,
        Frequency.Quarterly,
        Calendar.target(),
        BusinessDayConvention.Unadjusted,
    )
    return Schedule(*args) if rule is None else Schedule(*args, rule)


def interior_dates(schedule):
    return [schedule.date(i) for i in range(1, schedule.size() - 1)]


def test_twentieth_imm_rule_puts_every_interior_date_on_an_imm_twentieth():
    schedule = a_quarterly_schedule(DateGeneration.TwentiethIMM)
    stamps = {(date.day, date.month) for date in interior_dates(schedule)}
    assert stamps == {(20, month) for month in IMM_MONTHS}


def test_the_default_rule_is_forward_and_leaves_the_start_date_untouched():
    schedule = a_quarterly_schedule()
    assert {date.day for date in interior_dates(schedule)} == {TODAY.day}
    assert schedule.date(0) == TODAY


def test_an_explicit_forward_rule_reproduces_the_default():
    default = a_quarterly_schedule()
    forward = a_quarterly_schedule(DateGeneration.Forward)
    assert default.size() == forward.size()
    assert interior_dates(default) == interior_dates(forward)


class Market:
    """The quote, curve and conventions a spread helper is built against."""

    def __init__(self):
        self.settings = Settings()
        self.settings.set_evaluation_date(TODAY)
        self.calendar = Calendar.target()
        self.day_counter = DayCounter.actual360()
        self.quote = SimpleQuote(0.0120)
        self.discount = FlatForward(TODAY, 0.06, DayCounter.actual365_fixed())

    def helper(self, rule):
        return SpreadCdsHelper(
            self.quote,
            Period(5, "Years"),
            1,
            self.calendar,
            Frequency.Quarterly,
            BusinessDayConvention.Following,
            rule,
            self.day_counter,
            0.4,
            self.discount,
            self.settings,
        )


def test_a_spread_helper_builds_under_the_twentieth_imm_rule():
    helper = Market().helper(DateGeneration.TwentiethIMM)
    assert helper.pillar_date() == helper.latest_date()
    assert helper.pillar_date().month in IMM_MONTHS
    assert helper.pillar_date().day == 20


@pytest.mark.parametrize(
    "rule", [DateGeneration.OldCDS, DateGeneration.CDS, DateGeneration.CDS2015]
)
def test_the_post_big_bang_rules_roll_the_maturity_to_a_cds_date(rule):
    """Five years quoted on 15 May 2007 rolls to the twentieth of the IMM month
    a quarter past the anniversary of the previous roll date, which is a
    different date from the 15 May 2012 a dropped rule would produce."""
    assert Market().helper(rule).latest_date() == Date(20, 6, 2012)


def test_include_todays_cash_flows_round_trips_its_three_states():
    settings = Settings()
    assert settings.include_todays_cash_flows() is None
    settings.set_include_todays_cash_flows(True)
    assert settings.include_todays_cash_flows() is True
    settings.set_include_todays_cash_flows(False)
    assert settings.include_todays_cash_flows() is False
    settings.set_include_todays_cash_flows(None)
    assert settings.include_todays_cash_flows() is None
