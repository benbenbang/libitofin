"""The floating ibor leg facade (#626): a leg built directly from Python lays
its coupons out on a schedule.

This file is STRUCTURAL. The leg's coupons are not exposed - they exist to be
consumed by the raw ``CapFloor.cap`` / ``floor`` / ``collar`` constructors - so
what is checkable here is the coupon count, the required notional and the
setter semantics. The numeric oracle for the leg is in test_capfloor.py, where
the same builder reproduces the core's cached cap and floor NPVs to 1e-11; a
leg that laid its coupons out wrongly could not hit those literals.

What is pinned here:

A. A 5Y semiannual schedule yields one coupon per schedule period.
B. A leg with no notional raises rather than building a coupon-less leg: the
   core reports "no notional given" and the facade surfaces it at
   ``coupon_count`` rather than at construction, since the core builder is
   consumed only when the coupons are read.
C. The setters return a NEW leg and leave the receiver alone. This is the shape
   the core's consumed-self chain has and the reason a leg can be bound to a
   name and reused; a mutate-in-place facade would let an earlier binding
   silently change under a later call.
"""

import pytest

from itofin import ItofinError, Settings
from itofin.cashflows import IborLeg
from itofin.indexes import Euribor
from itofin.termstructures import FlatForward
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    DayCounter,
    Frequency,
    Schedule,
)

EVAL = Date(15, 1, 2026)
END = Date(15, 1, 2031)
PERIODS = 10

SETTINGS = Settings()
SETTINGS.set_evaluation_date(EVAL)


def _index():
    curve = FlatForward(EVAL, 0.05, DayCounter.actual365_fixed())
    return Euribor.six_months(curve, SETTINGS)


def _schedule():
    return Schedule(
        EVAL,
        END,
        Frequency.Semiannual,
        Calendar.target(),
        BusinessDayConvention.ModifiedFollowing,
    )


def _leg():
    return IborLeg(_schedule(), _index()).with_notional(100.0)


def test_the_leg_builds_one_coupon_per_schedule_period():
    schedule = _schedule()
    assert schedule.size() == PERIODS + 1
    leg = IborLeg(schedule, _index()).with_notional(100.0)
    assert leg.coupon_count() == PERIODS


def test_a_leg_without_a_notional_raises():
    leg = IborLeg(_schedule(), _index())
    with pytest.raises(ItofinError) as raised:
        leg.coupon_count()
    assert "notional" in str(raised.value)


def test_the_optional_setters_leave_the_coupon_count_alone():
    """They configure how each coupon accrues and fixes, not how many there
    are: the count is the schedule's. What they do reach is pinned numerically
    in test_capfloor.py, where the cached NPV moves with all three."""
    configured = (
        _leg()
        .with_payment_day_counter(DayCounter.actual360())
        .with_payment_adjustment(BusinessDayConvention.ModifiedFollowing)
        .with_fixing_days(2)
    )
    assert configured.coupon_count() == PERIODS


def test_a_setter_returns_a_new_leg_and_leaves_the_receiver_alone():
    bare = IborLeg(_schedule(), _index())
    configured = bare.with_notional(100.0)

    assert configured is not bare
    assert configured.coupon_count() == PERIODS
    with pytest.raises(ItofinError):
        bare.coupon_count()
