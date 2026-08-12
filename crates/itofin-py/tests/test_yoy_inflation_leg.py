"""The year-on-year coupon and leg facades (#848): a leg built directly from
Python lays its coupons out on a schedule and rates each one off the linked
curve.

Fixture. The bootstrapped year-on-year curve of test_yoy_inflation_reprice.py,
which that file pins end to end: it is 13 August 2007, UK RPI carries the
thirty-one monthly figures published from January 2005, and fifteen quoted swaps
fit a Monthly curve the *ratio* year-on-year index is then linked onto. Only the
curve is reused here; nothing about the leg is.

The oracle. `rate()` is NOT checked against `gearing * coupon.index_fixing() +
spread`: that is the swaplet pricer's own expression, so both sides would break
together. It is checked against the index read at a date this file derives
itself, `index.fixing(<the first of the month the accrual end minus the lag
falls in>)`, with the observation dates written out as literals. That pins three
things the pricer could get wrong independently - that the observation lags off
the accrual *end* rather than off `fixing_date()`, that the lag is subtracted
before the period snap, and that the snap is Monthly - while sharing only the
forecast machinery, which the reprice fixture already pins.

The accrual-end anchor needs its own coupon to show at all. With no fixing days
the fixing date and the lagged accrual end are the same day, so the two rules
agree by accident; the leg in
test_the_rate_lags_off_the_accrual_end_not_off_the_fixing_date pushes them into
different months.

Omitted visibly: caps and floors, which the facade does not expose (a capped
coupon needs a pricer carrying an optionlet volatility, and #838's cap/floor
instrument is the supported route), and the erased `build()` leg.
"""

import pytest

from itofin import ItofinError, Settings
from itofin.cashflows import YoYInflationLeg
from itofin.indexes import CpiInterpolationType, YoYInflationIndex, ZeroInflationIndex
from itofin.instruments import SwapType, YearOnYearInflationSwap
from itofin.pricingengines import DiscountingSwapEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    FlatForward,
    PiecewiseYoYInflationCurve,
    YearOnYearInflationSwapHelper,
)
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

TODAY = Calendar.united_kingdom().adjust(
    Date(13, 8, 2007), BusinessDayConvention.Following
)
CURVE_BASE = Date(1, 7, 2007)
LAG = Period(2, "Months")
MATURITY = Date(13, 8, 2010)
NOMINAL_RATE = 0.05

NOTIONAL = 2_500_000.0
GEARING = 1.5
SPREAD = 0.002

FIX_DATA = [
    189.9, 189.9, 189.6, 190.5, 191.6, 192.0, 192.2, 192.2, 192.6, 193.1,
    193.3, 193.6, 194.1, 193.4, 194.2, 195.0, 196.5, 197.7, 198.5, 198.5,
    199.2, 200.1, 200.4, 201.1, 202.7, 201.6, 203.1, 204.4, 205.4, 206.2,
    207.3,
]

YY_DATA = [
    (Date(13, 8, 2008), 2.95),
    (Date(13, 8, 2009), 2.95),
    (Date(13, 8, 2010), 2.93),
    (Date(15, 8, 2011), 2.955),
    (Date(13, 8, 2012), 2.945),
    (Date(13, 8, 2013), 2.985),
    (Date(13, 8, 2014), 3.01),
    (Date(13, 8, 2015), 3.035),
    (Date(13, 8, 2016), 3.055),
    (Date(13, 8, 2017), 3.075),
    (Date(13, 8, 2019), 3.105),
    (Date(15, 8, 2022), 3.135),
    (Date(13, 8, 2027), 3.155),
    (Date(13, 8, 2032), 3.145),
    (Date(13, 8, 2037), 3.145),
]

ACCRUAL_ENDS = [Date(13, 8, 2008), Date(13, 8, 2009), Date(13, 8, 2010)]

OBSERVED_PERIOD_STARTS = [Date(1, 6, 2008), Date(1, 6, 2009), Date(1, 6, 2010)]
"""Where each coupon's rate resolves, derived here rather than read off the
coupon: the accrual end back two months (13 August -> 13 June), then the first of
the month that lands in, because the index publishes Monthly. Every one of them
is past the July 2007 end of the RPI history, so all three forecast off the
curve rather than reading a stored figure."""


class Fixture:
    """The bootstrapped curve with its index, and the day count and calendar
    every leg and swap below shares."""

    def __init__(self):
        self.settings = Settings()
        self.settings.set_evaluation_date(TODAY)
        self.calendar = Calendar.united_kingdom()
        self.day_counter = DayCounter.thirty360_bond_basis()

        rpi = ZeroInflationIndex.uk_rpi(self.settings)
        for i, fixing in enumerate(FIX_DATA):
            rpi.add_fixing(Date(1, i % 12 + 1, 2005 + i // 12), fixing)
        self.index = YoYInflationIndex.from_underlying(rpi)

        self.nominal = FlatForward(
            Date(13, 8, 2007), NOMINAL_RATE, DayCounter.actual360()
        )
        self.curve = PiecewiseYoYInflationCurve(
            TODAY,
            CURVE_BASE,
            YY_DATA[0][1] / 100.0,
            Frequency.Monthly,
            self.day_counter,
            self._helpers(),
        )
        self.index.link_to(self.curve)

        self.schedule = Schedule(
            self.nominal.reference_date(),
            MATURITY,
            Frequency.Annual,
            self.calendar,
            BusinessDayConvention.Unadjusted,
            DateGeneration.Backward,
        )

    def _helpers(self):
        return [
            YearOnYearInflationSwapHelper(
                SimpleQuote(rate / 100.0),
                LAG,
                maturity,
                self.calendar,
                BusinessDayConvention.ModifiedFollowing,
                self.day_counter,
                self.index,
                CpiInterpolationType.Flat,
                self.nominal,
                self.settings,
            )
            for maturity, rate in YY_DATA
        ]

    def leg(self, **overrides) -> YoYInflationLeg:
        """A leg on the fixture schedule; the keywords go straight through."""
        return YoYInflationLeg(
            self.schedule,
            self.calendar,
            self.index,
            LAG,
            CpiInterpolationType.Flat,
            self.day_counter,
            **overrides,
        )


@pytest.fixture
def fixture() -> Fixture:
    return Fixture()


def test_the_leg_lays_its_coupons_out_on_the_schedule(fixture):
    """One coupon per schedule period, each accruing between consecutive
    schedule dates and paying on its accrual end rolled on the payment calendar,
    and each carrying the notional, gearing and spread the builder was given.

    The coupons are bound once: the leg rebuilds them on every call, so reading
    through two calls would compare different objects.

    Period and DayCounter carry no __eq__, so those two go through repr."""
    coupons = fixture.leg(notional=NOTIONAL, gearing=GEARING, spread=SPREAD).coupons()

    assert len(coupons) == fixture.schedule.size() - 1 == 3

    for i, coupon in enumerate(coupons):
        assert coupon.accrual_start_date() == fixture.schedule.date(i)
        assert coupon.accrual_end_date() == fixture.schedule.date(i + 1)
        assert coupon.accrual_end_date() == ACCRUAL_ENDS[i]
        assert coupon.date() == fixture.calendar.adjust(
            coupon.accrual_end_date(), BusinessDayConvention.ModifiedFollowing
        )
        assert coupon.nominal() == NOTIONAL
        assert coupon.gearing() == GEARING
        assert coupon.spread() == SPREAD
        assert coupon.interpolation() == CpiInterpolationType.Flat
        assert coupon.fixing_days() == 0
        assert repr(coupon.observation_lag()) == repr(LAG)
        assert repr(coupon.day_counter()) == repr(fixture.day_counter)


def test_each_coupon_rates_the_fixing_of_the_period_its_accrual_ends_in(fixture):
    """The oracle. Each coupon's rate is the index read at a date this file
    derives from the schedule, geared and spread - not at the coupon's own
    reported observation, which would only restate the pricer's arithmetic.

    The gearing is not 1 and the spread is not 0, so a pricer dropping either
    would show. The third coupon's fixing is a genuinely interpolated curve
    value; the first two sit on the seeded front quote.

    The comparison is exact. Both sides run the same two floating-point
    operations in the same order over the same in-process index read, so a
    tolerance here would only hide a real difference."""
    coupons = fixture.leg(notional=NOTIONAL, gearing=GEARING, spread=SPREAD).coupons()

    for coupon, observed in zip(coupons, OBSERVED_PERIOD_STARTS, strict=True):
        expected = GEARING * fixture.index.fixing(observed) + SPREAD
        assert coupon.rate() == expected
        assert coupon.amount() == coupon.rate() * coupon.accrual_period() * (
            coupon.nominal()
        )

    assert fixture.index.fixing(OBSERVED_PERIOD_STARTS[2]) != pytest.approx(
        fixture.index.fixing(OBSERVED_PERIOD_STARTS[0]), abs=1e-9
    ), "the last coupon must read a solved pillar, or the oracle only ever sees the seeded front quote"


def test_the_rate_lags_off_the_accrual_end_not_off_the_fixing_date(fixture):
    """A year-on-year coupon overrides the base rule that reads the index at the
    fixing date: it lags off the accrual end instead. With no fixing days the
    two dates coincide, so the leg here rolls the fixing date fifteen days back
    into the previous month, where the curve carries a different rate. The rate
    stays on the June figure."""
    coupon = fixture.leg(notional=NOTIONAL, fixing_days=15).coupons()[2]

    assert coupon.fixing_date() == Date(29, 5, 2010)
    assert coupon.index_fixing() == fixture.index.fixing(Date(1, 6, 2010))
    assert fixture.index.fixing(Date(1, 5, 2010)) != pytest.approx(
        fixture.index.fixing(Date(1, 6, 2010)), abs=1e-9
    ), "the two months must differ, or reading the fixing date would pass by accident"


def test_the_discounted_coupons_are_the_swaps_year_on_year_leg(fixture):
    """A plain leg, discounted coupon by coupon on the nominal curve, is the
    year-on-year leg of the swap built over the same schedule and index. The
    swap reaches its leg through the core builder this facade wraps, so the two
    agreeing pins the wiring: the notional broadcast, the payment roll and the
    discounting all have to line up.

    Discounting to the curve reference date is exact here because the engine's
    npv date defaults to it, leaving nothing to divide out."""
    swap_notional = 1_000_000.0
    coupons = fixture.leg(notional=swap_notional).coupons()
    discounted = sum(
        coupon.amount() * fixture.nominal.discount_date(coupon.date(), True)
        for coupon in coupons
    )

    swap = YearOnYearInflationSwap(
        SwapType.Payer,
        swap_notional,
        fixture.schedule,
        0.03,
        fixture.day_counter,
        fixture.schedule,
        fixture.index,
        LAG,
        CpiInterpolationType.Flat,
        0.0,
        fixture.day_counter,
        fixture.calendar,
        BusinessDayConvention.ModifiedFollowing,
        fixture.settings,
    )
    swap.set_engine(DiscountingSwapEngine(fixture.nominal, fixture.settings))

    print(f"discounted leg = {discounted!r}, swap yoy leg = {swap.yoy_leg_npv()!r}")
    assert discounted == pytest.approx(swap.yoy_leg_npv(), abs=1e-8)


def test_a_leg_with_no_notional_is_refused(fixture):
    """The notional is the one setting the facade leaves optional that the core
    still insists on, because a per-coupon notionals list is the other way to
    give it. Neither given surfaces at coupons() time."""
    with pytest.raises(ItofinError) as raised:
        fixture.leg().coupons()
    assert "no notional given" in str(raised.value)


def test_a_notional_per_coupon_carries_over_to_the_rest(fixture):
    """A notionals list shorter than the leg leaves the last entry standing for
    every coupon past it, which is how the core broadcasts every per-coupon
    setting."""
    coupons = fixture.leg(notionals=[1_000_000.0, 2_000_000.0]).coupons()

    assert [coupon.nominal() for coupon in coupons] == [
        1_000_000.0,
        2_000_000.0,
        2_000_000.0,
    ]
