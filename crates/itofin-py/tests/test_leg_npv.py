"""The erased YoY leg and the leg-level NPV (#878): YoYInflationLeg.build()
hands back a Leg of CashFlows, and cashflows.npv sums it on a discount curve.

The oracle is a hand-discounted replication: npv(leg, curve, settings) must
equal the sum over the leg of amount(i) * curve.discount_date(date(i)) within
1e-10. The npv_date defaults to the settlement date, itself defaulting to the
evaluation date the curve is referenced at, so the normalizing division by
discount(npv_date) is by exactly 1.0 and the replication needs no adjustment.
This is the leg-level sum the capped/floored path (#863) deliberately routed
around by asserting per-coupon rate() instead.

The fixture is the flat year-on-year shape of test_instrument_ergonomics.py:
a quoted YoY index on UK RPI's metadata linked to a two-node flat 3% curve
(linear interpolation between equal rates is that rate everywhere), evaluated
at 13-Aug-2007, discounted on a flat 5% Actual/360 curve. Copied rather than
imported: test files stay self-contained.

Also pinned: the structural contract of the wrappers (one flow per schedule
period, positive amounts, strictly ascending dates, negative indexing, the
IndexError boundary) and that an explicit settlement_date equal to the
evaluation date reproduces the default-arguments result exactly.
"""

import pytest

from itofin import ItofinError, Settings
from itofin.cashflows import YoYInflationLeg, npv
from itofin.indexes import CpiInterpolationType, YoYInflationIndex
from itofin.termstructures import FlatForward, InterpolatedYoYInflationCurve
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    DateGeneration,
    DayCounter,
    Frequency,
    Period,
    Schedule,
)

TODAY = Date(13, 8, 2007)
BASE = Date(1, 7, 2007)
END = Date(13, 8, 2012)
UK = Calendar.united_kingdom()
THIRTY_360 = DayCounter.thirty360_bond_basis()
FLAT_YOY = 0.03
NOTIONAL = 1_000_000.0
PERIODS = 5


def _settings():
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    return settings


def _yoy_index(settings):
    index = YoYInflationIndex(
        "YY_RPI",
        "UK",
        "GB",
        False,
        Frequency.Monthly,
        Period(1, "Months"),
        "British pound sterling",
        "GBP",
        826,
        "£",
        "p",
        100,
        settings,
    )
    index.link_to(
        InterpolatedYoYInflationCurve(
            TODAY,
            [BASE, Date(1, 1, 2040)],
            [FLAT_YOY, FLAT_YOY],
            Frequency.Monthly,
            THIRTY_360,
        )
    )
    return index


def _leg_builder(settings):
    schedule = Schedule(
        TODAY,
        END,
        Frequency.Annual,
        UK,
        BusinessDayConvention.Unadjusted,
        DateGeneration.Backward,
    )
    return YoYInflationLeg(
        schedule,
        UK,
        _yoy_index(settings),
        Period(2, "Months"),
        CpiInterpolationType.Flat,
        THIRTY_360,
        notional=NOTIONAL,
    )


def _discount_curve():
    return FlatForward(TODAY, 0.05, DayCounter.actual360())


def test_the_built_leg_has_one_flow_per_period_paying_forward():
    settings = _settings()
    leg = _leg_builder(settings).build()

    assert len(leg) == PERIODS
    dates = [leg[i].date() for i in range(len(leg))]
    for earlier, later in zip(dates, dates[1:]):
        assert (later - earlier) > 0
    for i in range(len(leg)):
        assert leg[i].amount() > 0.0


def test_the_leg_indexes_from_the_end_and_bounds_are_errors():
    settings = _settings()
    leg = _leg_builder(settings).build()

    assert leg[-1].date() == leg[PERIODS - 1].date()
    with pytest.raises(IndexError):
        leg[PERIODS]
    with pytest.raises(IndexError):
        leg[-PERIODS - 1]


def test_npv_equals_the_hand_discounted_sum():
    settings = _settings()
    leg = _leg_builder(settings).build()
    curve = _discount_curve()

    summed = npv(leg, curve, settings)
    by_hand = sum(
        leg[i].amount() * curve.discount_date(leg[i].date()) for i in range(len(leg))
    )
    print(f"\nnpv = {summed!r}\nhand-discounted sum = {by_hand!r}")
    assert summed != 0.0
    assert abs(summed - by_hand) < 1.0e-10


def test_an_explicit_settlement_at_eval_matches_the_defaults():
    settings = _settings()
    builder = _leg_builder(settings)
    curve = _discount_curve()

    defaulted = npv(builder.build(), curve, settings)
    explicit = npv(builder.build(), curve, settings, settlement_date=TODAY)
    assert explicit == defaulted


def test_npv_without_an_evaluation_date_raises():
    settings = _settings()
    leg = _leg_builder(settings).build()

    bare = Settings()
    with pytest.raises(ItofinError):
        npv(leg, _discount_curve(), bare)
