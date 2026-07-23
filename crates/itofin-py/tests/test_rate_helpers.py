"""Rate-helper, generalized-Euribor and Calendar-arithmetic facades (#528).

The API surface T5's bootstrap fixture (piecewiseyieldcurve.rs:333-360) is built
from: `Calendar.adjust`/`advance` for the reference and settlement dates, the
general `Euribor(tenor, curve_or_None, settings)` constructor with `fixing`, and
the deposit/swap rate helpers with their `maturity_date`/`pillar_date`/
`implied_quote` inspectors. Independent expectations are computed here from
Python's stdlib or from the documented Euribor conventions, never read back off
the helper.
"""

import datetime

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import Euribor
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    DepositRateHelper,
    FlatForward,
    SwapRateHelper,
)
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    DayCounter,
    Frequency,
    Period,
)


def _settings_today():
    """The T5 fixture head: TARGET, evaluation date = adjust(15-Jun-2026)."""
    calendar = Calendar.target()
    today = calendar.adjust(Date(15, 6, 2026), BusinessDayConvention.Following)
    settings = Settings()
    settings.set_evaluation_date(today)
    return settings, calendar, today


def _flat_curve(reference: Date):
    return FlatForward(reference, 0.03, DayCounter.actual365_fixed())


# --- an independent TARGET oracle from Python's stdlib (June 2026 and mid-Jan
# --- 2028 are both free of TARGET holidays, so only weekends move a date). ----


def _is_weekend(d: datetime.date) -> bool:
    return d.weekday() >= 5


def _following(d: datetime.date) -> datetime.date:
    while _is_weekend(d):
        d += datetime.timedelta(days=1)
    return d


def _advance_business_days(d: datetime.date, n: int) -> datetime.date:
    step = 1 if n >= 0 else -1
    remaining = abs(n)
    while remaining > 0:
        d += datetime.timedelta(days=step)
        if not _is_weekend(d):
            remaining -= 1
    return d


def _same_day(got: Date, expected: datetime.date) -> bool:
    return (got.year, got.month, got.day) == (
        expected.year,
        expected.month,
        expected.day,
    )


def test_adjust_rolls_a_weekend_to_the_following_business_day():
    calendar = Calendar.target()
    expected = _following(datetime.date(2028, 1, 15))
    got = calendar.adjust(Date(15, 1, 2028), BusinessDayConvention.Following)
    assert _same_day(got, expected)


def test_advance_matches_business_day_arithmetic():
    calendar = Calendar.target()
    today = calendar.adjust(Date(15, 6, 2026), BusinessDayConvention.Following)
    today_dt = _following(datetime.date(2026, 6, 15))
    assert _same_day(today, today_dt)

    expected = _advance_business_days(today_dt, 2)
    got = calendar.advance(today, 2, "Days", BusinessDayConvention.Following, False)
    assert _same_day(got, expected)


def test_advance_unknown_unit_raises():
    calendar = Calendar.target()
    with pytest.raises(ItofinError):
        calendar.advance(
            Date(15, 6, 2026), 2, "Fortnights", BusinessDayConvention.Following, False
        )


def test_euribor_general_ctor_builds_over_empty_handle():
    settings, _, _ = _settings_today()
    index = Euribor(Period(3, "Months"), None, settings)
    assert index is not None


def test_euribor_general_ctor_builds_over_a_curve():
    settings, _, today = _settings_today()
    index = Euribor(Period(6, "Months"), _flat_curve(today), settings)
    assert index is not None


def test_euribor_daily_tenor_is_rejected():
    settings, _, _ = _settings_today()
    with pytest.raises(ItofinError):
        Euribor(Period(1, "Days"), None, settings)


def test_fixing_off_a_curve_is_a_finite_rate():
    settings, _, today = _settings_today()
    index = Euribor(Period(3, "Months"), _flat_curve(today), settings)
    rate = index.fixing(today, False)
    assert rate == rate
    assert -1.0 < rate < 1.0


def test_fixing_without_a_forwarding_curve_raises():
    settings, _, today = _settings_today()
    index = Euribor(Period(3, "Months"), None, settings)
    with pytest.raises(ItofinError):
        index.fixing(today, False)


def test_deposit_helper_constructs_both_ways():
    settings, _, _ = _settings_today()
    index = Euribor(Period(3, "Months"), None, settings)
    assert DepositRateHelper(SimpleQuote(0.04557), index) is not None
    assert DepositRateHelper.from_rate(0.04557, index) is not None


def test_deposit_helper_maturity_and_pillar_dates():
    settings, calendar, today = _settings_today()
    index = Euribor(Period(3, "Months"), None, settings)
    helper = DepositRateHelper(SimpleQuote(0.04557), index)

    # Independent chain (deposit initialize_dates + IborIndex.maturity_date):
    # value date = today + 2 fixing days (Following); maturity = value + 3M under
    # the 3M Euribor convention (ModifiedFollowing, end-of-month).
    value_date = calendar.advance(today, 2, "Days", BusinessDayConvention.Following, False)
    expected_maturity = calendar.advance(
        value_date, 3, "Months", BusinessDayConvention.ModifiedFollowing, True
    )
    assert helper.maturity_date() == expected_maturity
    assert helper.pillar_date() == expected_maturity


def test_quote_form_retains_the_caller_quote_object():
    settings, _, _ = _settings_today()
    index = Euribor(Period(3, "Months"), None, settings)
    quote = SimpleQuote(0.04557)
    helper = DepositRateHelper(quote, index)

    assert helper.quote_value() == pytest.approx(0.04557)
    quote.set_value(0.06)
    # The same object is wired: mutating the caller's quote moves the helper's.
    assert helper.quote_value() == pytest.approx(0.06)


def test_from_rate_does_not_retain_a_mutable_quote():
    settings, _, _ = _settings_today()
    index = Euribor(Period(3, "Months"), None, settings)
    helper = DepositRateHelper.from_rate(0.04557, index)
    assert helper.quote_value() == pytest.approx(0.04557)


def test_implied_quote_without_a_curve_raises():
    settings, _, _ = _settings_today()
    index = Euribor(Period(3, "Months"), None, settings)
    helper = DepositRateHelper(SimpleQuote(0.04557), index)
    with pytest.raises(ItofinError):
        helper.implied_quote()


def test_swap_helper_constructs_and_reports_dates():
    settings, calendar, today = _settings_today()
    euribor6m = Euribor(Period(6, "Months"), None, settings)
    helper = SwapRateHelper(
        SimpleQuote(0.05),
        Period(2, "Years"),
        calendar,
        Frequency.Annual,
        BusinessDayConvention.Unadjusted,
        DayCounter.thirty360_bond_basis(),
        euribor6m,
    )
    assert helper is not None

    # A 2-year swap settling ~2 business days after 15-Jun-2026 matures in 2028.
    maturity = helper.maturity_date()
    assert maturity.year == 2028
    assert helper.pillar_date().year == 2028
