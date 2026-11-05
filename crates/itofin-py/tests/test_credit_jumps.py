"""Survival jump boundaries, quote retention, and moving reference dates."""

import gc
import math

import pytest

from itofin import ItofinError, Settings
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward, FlatHazardRate, PiecewiseDefaultCurve, SpreadCdsHelper
from itofin.time import BusinessDayConvention, Calendar, Date, DateGeneration, DayCounter, Frequency, Period


def test_jumps_are_strictly_after_dates_and_observe_retained_quotes():
    today = Date(15, 6, 2026)
    jump_date = today + 365
    jump = SimpleQuote(0.8)
    curve = FlatHazardRate.with_jumps(today, SimpleQuote(0.02), DayCounter.actual365_fixed(), [jump], [jump_date])
    assert curve.jump_dates() == [jump_date]
    assert curve.jump_times() == [1.0]
    assert curve.survival_probability(1.0) == pytest.approx(math.exp(-0.02), rel=0, abs=1e-14)
    assert curve.survival_probability(2.0) == pytest.approx(0.8 * math.exp(-0.04), rel=0, abs=1e-14)
    jump.set_value(0.9)
    del jump
    gc.collect()
    assert curve.survival_probability(2.0) == pytest.approx(0.9 * math.exp(-0.04), rel=0, abs=1e-14)
    copied = curve.jump_dates()
    copied.clear()
    assert curve.jump_dates() == [jump_date]


def test_generated_jump_dates_stay_fixed_while_times_move():
    settings = Settings()
    today = Date(15, 6, 2026)
    settings.set_evaluation_date(today)
    curve = FlatHazardRate.moving_with_jumps(
        0,
        Calendar.null_calendar(),
        SimpleQuote(0.02),
        DayCounter.actual365_fixed(),
        settings,
        [SimpleQuote(0.9)],
    )
    dates = curve.jump_dates()
    assert dates == [Date(31, 12, 2026)]
    before = curve.jump_times()[0]
    settings.set_evaluation_date(today + 10)
    assert curve.jump_dates() == dates
    assert curve.jump_times()[0] == pytest.approx(before - 10 / 365, rel=0, abs=1e-14)


def test_invalid_jump_values_are_rejected_when_crossed():
    today = Date(15, 6, 2026)
    curve = FlatHazardRate.with_jumps(
        today, SimpleQuote(0.02), DayCounter.actual365_fixed(), [SimpleQuote(1.1)], [today + 365]
    )
    assert curve.survival_probability(1.0) > 0
    with pytest.raises(ItofinError):
        curve.survival_probability(2.0)
    with pytest.raises(ItofinError):
        FlatHazardRate.with_jumps(today, SimpleQuote(0.02), DayCounter.actual365_fixed(), [], [today + 365])


TODAY = Date(15, 6, 2026)
SPREADS = [0.3, 0.25, 0.22]
PROBES = [0.1, 1.0, 1.5, 2.0, 2.9]


def piecewise_helpers(settings, day_counter):
    """Fresh helpers per curve, on the Settings the curve is told about."""
    discount = FlatForward(TODAY, 0.03, day_counter)
    return [
        SpreadCdsHelper(
            SimpleQuote(spread),
            Period(tenor, "Years"),
            1,
            Calendar.target(),
            Frequency.Quarterly,
            BusinessDayConvention.Following,
            DateGeneration.TwentiethIMM,
            day_counter,
            0.4,
            discount,
            settings,
        )
        for tenor, spread in enumerate(SPREADS, start=1)
    ]


def dated_settings():
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    settings.set_include_todays_cash_flows(True)
    return settings


def unjumped_survival(curve, t):
    """The survival a curve's own solved nodes give with the jumps left out."""
    times, rates = curve.times(), curve.data()
    integral = sum(
        rates[i] * (min(t, times[i]) - times[i - 1])
        for i in range(1, len(times))
        if times[i - 1] < t
    )
    return math.exp(-integral)


def assert_same_curve(actual, expected, probes):
    assert [date for date, _ in actual.nodes()] == [date for date, _ in expected.nodes()]
    for (_, actual_rate), (_, expected_rate) in zip(actual.nodes(), expected.nodes()):
        assert actual_rate == pytest.approx(expected_rate, rel=0, abs=1e-10)
    for t in probes:
        assert actual.survival_probability(t) == pytest.approx(expected.survival_probability(t), rel=0, abs=1e-10)


def test_piecewise_jumps_apply_strictly_after_their_dates_and_enter_the_bootstrap():
    settings = dated_settings()
    dc = DayCounter.actual365_fixed()
    first, second = TODAY + 100, TODAY + 200
    curve = PiecewiseDefaultCurve.with_jumps(
        TODAY, piecewise_helpers(settings, dc), dc, [SimpleQuote(0.9), SimpleQuote(0.8)], [first, second]
    )
    assert isinstance(curve, PiecewiseDefaultCurve)
    assert curve.jump_dates() == [first, second]
    assert curve.jump_times() == pytest.approx([100 / 365, 200 / 365], rel=0, abs=1e-14)
    for t, factor in [(0.1, 1.0), (100 / 365, 1.0), (0.5, 0.9), (200 / 365, 0.9), (0.6, 0.72), (2.5, 0.72)]:
        ratio = curve.survival_probability(t) / unjumped_survival(curve, t)
        assert ratio == pytest.approx(factor, rel=0, abs=1e-13)
    plain = PiecewiseDefaultCurve(TODAY, piecewise_helpers(settings, dc), dc)
    assert plain.jump_dates() == []
    assert curve.data() != pytest.approx(plain.data(), rel=0, abs=1e-6)


def test_piecewise_generated_jump_dates_are_year_ends_from_the_reference_year():
    settings = dated_settings()
    dc = DayCounter.actual365_fixed()
    jumps = [SimpleQuote(0.9), SimpleQuote(0.8)]
    fixed = PiecewiseDefaultCurve.with_jumps(TODAY, piecewise_helpers(settings, dc), dc, jumps)
    moving = PiecewiseDefaultCurve.moving_with_jumps(
        0, Calendar.null_calendar(), piecewise_helpers(settings, dc), dc, settings, jumps
    )
    expected = [Date(31, 12, 2026), Date(31, 12, 2027)]
    assert fixed.jump_dates() == expected
    assert moving.jump_dates() == expected


def test_piecewise_jump_quote_is_retained_and_rebootstraps():
    settings = dated_settings()
    dc = DayCounter.actual365_fixed()
    dates = [TODAY + 100]
    jump = SimpleQuote(0.9)
    helpers = piecewise_helpers(settings, dc)
    curve = PiecewiseDefaultCurve.with_jumps(TODAY, helpers, dc, [jump], dates)
    before = curve.data()
    jump.set_value(0.85)
    del jump, helpers
    gc.collect()
    fresh = PiecewiseDefaultCurve.with_jumps(TODAY, piecewise_helpers(settings, dc), dc, [SimpleQuote(0.85)], dates)
    assert curve.data() != pytest.approx(before, rel=0, abs=1e-6)
    assert_same_curve(curve, fresh, PROBES)


def test_piecewise_moving_roll_matches_a_fresh_fixed_curve():
    settings = dated_settings()
    dc = DayCounter.actual365_fixed()
    calendar = Calendar.target()
    curve = PiecewiseDefaultCurve.moving(2, calendar, piecewise_helpers(settings, dc), dc, settings)
    assert isinstance(curve, PiecewiseDefaultCurve)
    first = curve.data()
    assert curve.dates()[0] == calendar.advance(TODAY, 2, "Days", BusinessDayConvention.Following, False)
    rolled = Date(15, 7, 2026)
    settings.set_evaluation_date(rolled)
    reference = calendar.advance(rolled, 2, "Days", BusinessDayConvention.Following, False)
    assert curve.dates()[0] == reference
    assert curve.data() != pytest.approx(first, rel=0, abs=1e-9)
    fresh = PiecewiseDefaultCurve(reference, piecewise_helpers(settings, dc), dc)
    assert_same_curve(curve, fresh, PROBES)


@pytest.mark.parametrize("explicit_dates", [False, True])
def test_piecewise_moving_jumped_roll_matches_a_fresh_fixed_curve(explicit_dates):
    settings = dated_settings()
    dc = DayCounter.actual365_fixed()
    calendar = Calendar.target()
    jumps = [SimpleQuote(0.9), SimpleQuote(0.8)]
    dates = [TODAY + 100, TODAY + 400] if explicit_dates else None
    curve = PiecewiseDefaultCurve.moving_with_jumps(
        2, calendar, piecewise_helpers(settings, dc), dc, settings, jumps, dates
    )
    jump_dates = curve.jump_dates()
    assert jump_dates == (dates if explicit_dates else [Date(31, 12, 2026), Date(31, 12, 2027)])
    assert curve.dates()[0] == calendar.advance(TODAY, 2, "Days", BusinessDayConvention.Following, False)
    before = curve.jump_times()
    rolled = Date(15, 7, 2026)
    settings.set_evaluation_date(rolled)
    reference = calendar.advance(rolled, 2, "Days", BusinessDayConvention.Following, False)
    after = curve.jump_times()
    assert curve.jump_dates() == jump_dates
    assert curve.dates()[0] == reference
    assert after[0] < before[0]
    fresh = PiecewiseDefaultCurve.with_jumps(reference, piecewise_helpers(settings, dc), dc, jumps, jump_dates)
    assert_same_curve(curve, fresh, PROBES + [(before[0] + after[0]) / 2])


def test_piecewise_moving_without_evaluation_date_raises_instead_of_using_a_clock():
    dc = DayCounter.actual365_fixed()
    settings = Settings()
    helper_settings = dated_settings()
    calendar = Calendar.null_calendar()
    with pytest.raises(ItofinError):
        PiecewiseDefaultCurve.moving_with_jumps(
            0, calendar, piecewise_helpers(helper_settings, dc), dc, settings, [SimpleQuote(0.9)]
        )
    jump_date = TODAY + 100
    jumped = PiecewiseDefaultCurve.moving_with_jumps(
        0, calendar, piecewise_helpers(helper_settings, dc), dc, settings, [SimpleQuote(0.9)], [jump_date]
    )
    assert jumped.jump_dates() == [jump_date]
    plain = PiecewiseDefaultCurve.moving(0, calendar, piecewise_helpers(helper_settings, dc), dc, settings)
    for curve in (jumped, plain):
        with pytest.raises(ItofinError):
            curve.survival_probability(1.0)
    with pytest.raises(ItofinError):
        jumped.jump_times()
    settings.set_evaluation_date(TODAY)
    assert jumped.jump_times() == pytest.approx([100 / 365], rel=0, abs=1e-14)
    assert jumped.survival_probability(1.0) > 0
    assert plain.survival_probability(1.0) > 0


def test_piecewise_constructors_reject_bad_arguments():
    settings = dated_settings()
    dc = DayCounter.actual365_fixed()
    calendar = Calendar.null_calendar()
    jump = SimpleQuote(0.9)
    helpers = piecewise_helpers(settings, dc)
    with pytest.raises(ItofinError):
        PiecewiseDefaultCurve.with_jumps(TODAY, [], dc, [jump], [TODAY + 100])
    with pytest.raises(ItofinError):
        PiecewiseDefaultCurve.moving(0, calendar, [], dc, settings)
    with pytest.raises(ItofinError):
        PiecewiseDefaultCurve.moving_with_jumps(0, calendar, [], dc, settings, [jump], [TODAY + 100])
    with pytest.raises(ItofinError):
        PiecewiseDefaultCurve.with_jumps(TODAY, helpers, dc, [jump], [TODAY + 100, TODAY + 200])
    with pytest.raises(ItofinError):
        PiecewiseDefaultCurve.moving_with_jumps(0, calendar, helpers, dc, settings, [jump], [TODAY + 100, TODAY + 200])
