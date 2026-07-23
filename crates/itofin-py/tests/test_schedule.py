"""Oracle for the Schedule facade (issue #499).

The expected dates were pinned against an independently-run Rust ``MakeSchedule``
probe (a throwaway ``cargo test`` that printed ``schedule.dates()`` and was then
reverted). Both endpoints matter: 15-Jan-2028 and 15-Jan-2033 are Saturdays, so
under TARGET + ModifiedFollowing they roll forward to Monday 17-Jan. The oracle
therefore pins 17-Jan, not the naive 15th quoted in the issue body.
"""

import pytest

from itofin import ItofinError
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    Frequency,
    Schedule,
)

START = Date(15, 1, 2028)
END = Date(15, 1, 2033)


def _fixed_leg():
    return Schedule(
        START,
        END,
        Frequency.Annual,
        Calendar.target(),
        BusinessDayConvention.ModifiedFollowing,
    )


def test_fixed_leg_size_and_endpoints():
    sched = _fixed_leg()
    # 5 annual periods -> 6 dates.
    assert sched.size() == 6
    # 15-Jan-2028 (Sat) and 15-Jan-2033 (Sat) both roll to Mon 17-Jan.
    assert sched.date(0) == Date(17, 1, 2028)
    assert sched.date(5) == Date(17, 1, 2033)


def test_fixed_leg_all_dates():
    sched = _fixed_leg()
    expected = [
        Date(17, 1, 2028),
        Date(15, 1, 2029),
        Date(15, 1, 2030),
        Date(15, 1, 2031),
        Date(15, 1, 2032),
        Date(17, 1, 2033),
    ]
    assert sched.dates() == expected


def test_float_leg_has_twice_the_periods():
    fixed = _fixed_leg()
    floating = Schedule(
        START,
        END,
        Frequency.Semiannual,
        Calendar.target(),
        BusinessDayConvention.ModifiedFollowing,
    )
    # Semiannual over the same span: 10 periods (11 dates) vs the fixed leg's 5.
    assert floating.size() == 11
    assert (floating.size() - 1) == 2 * (fixed.size() - 1)


def test_start_after_end_raises_not_panics():
    with pytest.raises(ItofinError):
        Schedule(
            END,
            START,
            Frequency.Annual,
            Calendar.target(),
            BusinessDayConvention.ModifiedFollowing,
        )


def test_zero_length_span_raises():
    with pytest.raises(ItofinError):
        Schedule(
            START,
            START,
            Frequency.Annual,
            Calendar.target(),
            BusinessDayConvention.ModifiedFollowing,
        )


def test_date_index_out_of_range_raises():
    sched = _fixed_leg()
    with pytest.raises(ItofinError):
        sched.date(6)
