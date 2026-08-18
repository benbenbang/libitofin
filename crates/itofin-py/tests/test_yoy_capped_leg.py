"""The capped/floored year-on-year leg path (#863): a leg given caps or floors
hands back coupons whose optionlets an optionlet pricer values.

Fixture. The Rust coupon oracle `capflooredyoyinflationcoupon_oracle.rs`,
transcribed. It is 10 February 2022 and UK YY_RPI has published exactly two
year-on-year figures, May 2021 at 2.81% and November 2021 at 2.935%. The
volatility surface observes inflation eight months back, so its base date is
1 June 2021; the coupons observe three months back. That gap is the whole point
of the fixture: a coupon whose fixing lands on or before the surface's base date
is *determined* and priced as pure intrinsic value with no volatility read at
all, while one fixing after it is priced under a distribution even though its
fixing is already history. No forecast curve appears - every figure read is
published - so the numbers isolate the coupon arithmetic.

Reaching the fixture. The Rust oracle builds its coupon directly; Python has no
standalone coupon constructor, so this file reaches the same coupon through
YoYInflationLeg.capped_floored_coupons(). That only reproduces the oracle if the
leg's single coupon accrues exactly the year ending on the anchored date: with
only two published figures, one day of schedule drift lands the observation in an
unpublished month and index_fixing() raises. Hence the single-period schedule
written out date by date, Unadjusted so nothing rolls, and the generation rule
passed explicitly - the Python Schedule facade defaults Forward where the core
MakeSchedule defaults Backward.

One Settings object stands behind both the index and the surface. Two would
resolve their dates against different evaluation dates and the rate would be
silently wrong.

The oracle. The determined arm is pinned by arithmetic this file derives, and
its rate() collapses onto the cap level, which is blind to a cap-the-whole-total
mis-wire - so effective_cap()/effective_floor(), the only direct pin on the
de-gearing and de-spreading, are asserted alongside. The live arm's three
distribution literals were probed off the Rust fixture built the direct way, so
reproducing them here checks the leg path rather than restating it. The
sum-identity and collar-identity the Rust oracle also carries are deliberately
NOT ported: both are blind to a consistently mis-wired strike.
"""

import pytest

from itofin import Settings
from itofin.cashflows import YoYInflationLeg, YoYInflationOptionletCouponPricer
from itofin.indexes import CpiInterpolationType, YoYInflationIndex
from itofin.termstructures import ConstantYoYOptionletVolatility
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

UK = Calendar.united_kingdom()
TODAY = Date(10, 2, 2022)
COUPON_LAG = Period(3, "Months")
SURFACE_LAG = Period(8, "Months")
BASE_DATE = Date(1, 6, 2021)

VOL = 0.01
NOMINAL = 1_000_000.0
GEARING = 2.5
SPREAD = 0.0035

DETERMINED_FIXING = 0.0281
"""The May 2021 figure, observed by the coupon accruing to DETERMINED_END."""
LIVE_FIXING = 0.02935
"""The November 2021 figure, observed by the coupon accruing to LIVE_END."""

DETERMINED_START, DETERMINED_END = Date(10, 8, 2020), Date(10, 8, 2021)
"""Fixing date 10 May 2021, on or before the surface's base date."""
LIVE_START, LIVE_END = Date(10, 2, 2021), Date(10, 2, 2022)
"""Fixing date 10 November 2021, after the surface's base date."""

DETERMINED_CAP = 0.04
DETERMINED_FLOOR = 0.091
"""Chosen so the effective floor is 0.035, the level the Rust oracle strikes its
determined floorlet at directly."""
LIVE_CAP = 0.09

LIVE_RATES = {
    "black": 0.076875,
    "unit_displaced": 0.07481625500957073,
    "bachelier": 0.07496638022514092,
}
"""rate() per distribution on the live capped coupon, probed off the Rust
fixture built the direct way (the Rust test asserts the caplet, which is not
bound here). Black is roughly -25 sigma, so its caplet underflows and its rate
is the bare swaplet 2.5 * 0.02935 + 0.0035; the other two are substantial."""

DISTRIBUTIONS = list(LIVE_RATES)


def ymd(date: Date) -> tuple[int, int, int]:
    """Date carries __eq__ but no ordering, so comparisons go through this."""
    return (date.year, date.month, date.day)


class Fixture:
    """The published index and the flat surface, sharing one Settings."""

    def __init__(self):
        self.settings = Settings()
        self.settings.set_evaluation_date(TODAY)
        self.index = YoYInflationIndex(
            "YY_RPI",
            "UK",
            "UK",
            False,
            Frequency.Monthly,
            Period(1, "Months"),
            "British pound sterling",
            "GBP",
            826,
            "£",
            "p",
            100,
            self.settings,
        )
        self.index.add_fixing(Date(1, 5, 2021), DETERMINED_FIXING)
        self.index.add_fixing(Date(1, 11, 2021), LIVE_FIXING)

        self.surface = ConstantYoYOptionletVolatility(
            VOL,
            0,
            UK,
            BusinessDayConvention.ModifiedFollowing,
            DayCounter.actual365_fixed(),
            SURFACE_LAG,
            Frequency.Monthly,
            False,
            -1.0,
            100.0,
            self.settings,
        )

    def leg(self, start: Date, end: Date, **overrides) -> YoYInflationLeg:
        """A leg of one coupon accruing start to end, geared and spread as the
        Rust fixture's coupon is."""
        schedule = Schedule(
            start,
            end,
            Frequency.Annual,
            UK,
            BusinessDayConvention.Unadjusted,
            DateGeneration.Backward,
        )
        return YoYInflationLeg(
            schedule,
            UK,
            self.index,
            COUPON_LAG,
            CpiInterpolationType.Flat,
            DayCounter.thirty360_bond_basis(),
            notional=NOMINAL,
            gearing=GEARING,
            spread=SPREAD,
            **overrides,
        )

    def pricer(self, distribution: str) -> YoYInflationOptionletCouponPricer:
        """A pricer over the flat surface, with no nominal curve: only the
        discounted price path would read one."""
        return getattr(YoYInflationOptionletCouponPricer, distribution)(self.surface)

    def coupon(self, start: Date, end: Date, distribution: str, **overrides):
        """The single capped/floored coupon of the leg, already priced."""
        leg = self.leg(start, end, **overrides)
        coupons = leg.capped_floored_coupons(self.pricer(distribution))
        assert len(coupons) == 1
        return coupons[0]


@pytest.fixture
def fixture() -> Fixture:
    return Fixture()


def test_the_leg_reaches_the_two_published_figures_across_the_base_date(fixture):
    """The fixture-integrity guard, and through the leg path it matters more
    than it does in Rust: it proves the schedule landed where the two figures
    are. The surface's base date separates the two coupons' fixing dates, and
    each coupon observes the figure the oracle says it does.

    These reads go through the plain coupons of the same leg configuration:
    a capped leg withholds the default pricer, so rate() would raise here, but
    fixing_date() and index_fixing() need none."""
    assert fixture.surface.base_date() == BASE_DATE

    determined = fixture.leg(
        DETERMINED_START, DETERMINED_END, caps=[DETERMINED_CAP]
    ).coupons()[0]
    live = fixture.leg(LIVE_START, LIVE_END, caps=[LIVE_CAP]).coupons()[0]

    assert determined.fixing_date() == Date(10, 5, 2021)
    assert live.fixing_date() == Date(10, 11, 2021)
    assert ymd(determined.fixing_date()) <= ymd(BASE_DATE)
    assert ymd(live.fixing_date()) > ymd(BASE_DATE)

    assert determined.index_fixing() == DETERMINED_FIXING
    assert live.index_fixing() == LIVE_FIXING


@pytest.mark.parametrize("distribution", DISTRIBUTIONS)
def test_a_determined_capped_coupon_pays_its_intrinsic_value(fixture, distribution):
    """A coupon fixing on or before the surface's base date is determined: its
    caplet is the intrinsic max(fixing - strike, 0), exactly, with no volatility
    read - which is why the distribution must not matter here.

    The swaplet is 2.5 * 0.0281 + 0.0035 = 0.07375 and the caplet
    2.5 * (0.0281 - 0.0146) = 0.03375, so the rate lands on the cap level. That
    collapse is blind to a pricer capping the geared, spread total instead of
    the de-geared strike, so the effective cap is asserted on its own."""
    coupon = fixture.coupon(
        DETERMINED_START, DETERMINED_END, distribution, caps=[DETERMINED_CAP]
    )

    assert coupon.is_capped() and not coupon.is_floored()
    assert coupon.effective_cap() == pytest.approx(
        (DETERMINED_CAP - SPREAD) / GEARING, abs=1e-15
    )
    assert coupon.effective_cap() == pytest.approx(0.0146, abs=1e-15)

    swaplet = GEARING * DETERMINED_FIXING + SPREAD
    caplet = GEARING * (DETERMINED_FIXING - coupon.effective_cap())
    print(f"determined {distribution}: rate = {coupon.rate()!r}")
    assert caplet > 0.0, "the caplet must be in the money, or a plain leg passes"
    assert coupon.rate() == pytest.approx(swaplet - caplet, abs=1e-12)
    assert coupon.rate() == pytest.approx(DETERMINED_CAP, abs=1e-12)
    assert coupon.amount() == pytest.approx(coupon.rate() * NOMINAL, abs=1e-8), (
        "a whole year on Thirty360 accrues exactly 1, so the amount is the rate "
        "on the nominal"
    )


@pytest.mark.parametrize("distribution", DISTRIBUTIONS)
def test_a_determined_floored_coupon_pays_its_intrinsic_value(fixture, distribution):
    """The floored twin, struck so the effective floor is the 0.035 the Rust
    oracle strikes its determined floorlet at: the floorlet is
    2.5 * (0.035 - 0.0281) = 0.01725 added to the same swaplet, landing on the
    floor level."""
    coupon = fixture.coupon(
        DETERMINED_START, DETERMINED_END, distribution, floors=[DETERMINED_FLOOR]
    )

    assert coupon.is_floored() and not coupon.is_capped()
    assert coupon.effective_floor() == pytest.approx(0.035, abs=1e-15)

    swaplet = GEARING * DETERMINED_FIXING + SPREAD
    floorlet = GEARING * (coupon.effective_floor() - DETERMINED_FIXING)
    print(f"determined floored {distribution}: rate = {coupon.rate()!r}")
    assert floorlet > 0.0, "the floorlet must be in the money"
    assert coupon.rate() == pytest.approx(swaplet + floorlet, abs=1e-12)
    assert coupon.rate() == pytest.approx(DETERMINED_FLOOR, abs=1e-12)


@pytest.mark.parametrize("distribution", DISTRIBUTIONS)
def test_a_live_capped_coupon_prices_under_its_own_distribution(fixture, distribution):
    """A coupon fixing after the base date reads the surface and routes to the
    formula its pricer names. The literals were probed off the Rust fixture
    built the direct way, so a leg that mis-lays its schedule, drops the pricer
    or reaches the wrong constructor misses them."""
    coupon = fixture.coupon(LIVE_START, LIVE_END, distribution, caps=[LIVE_CAP])

    assert coupon.effective_cap() == pytest.approx((LIVE_CAP - SPREAD) / GEARING)
    print(f"live {distribution}: rate = {coupon.rate()!r}")
    assert coupon.rate() == pytest.approx(LIVE_RATES[distribution], abs=1e-10)


def test_the_three_distributions_price_the_live_coupon_apart(fixture):
    """The surface-wiring pin. All three share one swaplet, so any difference in
    rate() is a difference in the caplet: a pricer that ignored the surface, or
    three that routed to one formula, could not tell them apart."""
    rates = [
        fixture.coupon(LIVE_START, LIVE_END, distribution, caps=[LIVE_CAP]).rate()
        for distribution in DISTRIBUTIONS
    ]

    for i, left in enumerate(rates):
        for right in rates[i + 1 :]:
            assert abs(left - right) > 1e-6, (
                f"two distributions agree to {abs(left - right)}, "
                "the fixture cannot tell them apart"
            )
