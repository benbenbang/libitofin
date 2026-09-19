"""Survival jump boundaries, quote retention, and moving reference dates."""

import gc
import math

import pytest

from itofin import ItofinError, Settings
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatHazardRate
from itofin.time import Calendar, Date, DayCounter


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
