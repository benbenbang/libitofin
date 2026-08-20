"""Price a year-on-year inflation swap and inspect its coupons one by one.

`inflation_swap.py` next door prices a `ZeroCouponInflationSwap`: one fixed flow
against one indexed flow, both exchanged at maturity. A
`YearOnYearInflationSwap` instead pays a *stream* of coupons, each rating the
year-on-year inflation observed over its own accrual period. That makes the
coupon the interesting object, so this example builds the leg twice: once inside
the swap, and once directly through the `YoYInflationLeg` facade, which hands
back `YoYInflationCoupon` objects you can interrogate.

Two readings that trip people up.

* `SwapType` names the *fixed* leg here, so a Payer pays fixed and receives
  inflation. That is the opposite reading from `ZeroCouponInflationSwap`, where
  it names the inflation leg.
* A year-on-year coupon overrides the base rule that reads the index at the
  fixing date: it lags off the *accrual end* instead, and the lag is subtracted
  before the period is snapped to the index's Monthly publication. With no
  fixing days the two dates coincide and the distinction is invisible, so the
  last section pushes them into different months to make it show.

The leg built standalone, discounted coupon by coupon, is the swap's own
year-on-year leg - which is what pins the two construction paths to each other.
Mirrors `crates/itofin-py/tests/test_yoy_inflation_leg.py`.

Fixture warning. `yoy_inflation_capfloor.py` next door runs a *different* UK RPI
fixture, and the two must not be crossed. This one files thirty-one published
figures with no sentinels, on the plain `Date(1, month, year)` walk, giving a
curve base date of 1 July 2007; the lag is two months everywhere, and the
nominal curve is Act/360. The cap/floor fixture adds two -999.0 sentinels, moves
the base to 1 August, observes zero lag on everything but its helpers and
discounts on Act/Act (ISDA).

Run it with:

    python example/python/yoy_inflation_swap.py
"""

# plugins
# itofin library
from itofin import Settings
from itofin.cashflows import YoYInflationLeg
from itofin.indexes import CpiInterpolationType, YoYInflationIndex, ZeroInflationIndex
from itofin.instruments import SwapType, YearOnYearInflationSwap
from itofin.pricingengines import DiscountingSwapEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward, PiecewiseYoYInflationCurve, YearOnYearInflationSwapHelper
from itofin.time import BusinessDayConvention, Calendar, Date, DateGeneration, DayCounter, Frequency, Period, Schedule

TODAY = Calendar.united_kingdom().adjust(Date(13, 8, 2007), BusinessDayConvention.Following)
CURVE_BASE = Date(1, 7, 2007)
LAG = Period(2, "Months")
MATURITY = Date(13, 8, 2010)
NOMINAL_RATE = 0.05
NOTIONAL = 1_000_000.0
FIXED_RATE = 0.03

# Thirty-one monthly UK RPI figures published from January 2005. No sentinels
# here: the last real figure, 207.3, is the July 2007 period, which is what
# makes 1 July 2007 the curve's base date.
FIX_DATA = [
    189.9, 189.9, 189.6, 190.5, 191.6, 192.0, 192.2, 192.2, 192.6, 193.1,
    193.3, 193.6, 194.1, 193.4, 194.2, 195.0, 196.5, 197.7, 198.5, 198.5,
    199.2, 200.1, 200.4, 201.1, 202.7, 201.6, 203.1, 204.4, 205.4, 206.2,
    207.3,
]

# (maturity, quoted year-on-year swap rate in percent).
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


class Market:
    """The bootstrapped year-on-year curve with its index, plus the day count,
    calendar and annual schedule every leg and swap below shares."""

    def __init__(self):
        self.settings = Settings()
        self.settings.set_evaluation_date(TODAY)
        self.calendar = Calendar.united_kingdom()
        self.day_counter = DayCounter.thirty360_bond_basis()

        rpi = ZeroInflationIndex.uk_rpi(self.settings)
        for i, fixing in enumerate(FIX_DATA):
            # A monthly index files each figure under the first of its month.
            rpi.add_fixing(Date(1, i % 12 + 1, 2005 + i // 12), fixing)
        # A ratio index derives its year-on-year rate from two RPI fixings a
        # year apart, so the history belongs on the underlying, not here.
        self.index = YoYInflationIndex.from_underlying(rpi)

        self.nominal = FlatForward(TODAY, NOMINAL_RATE, DayCounter.actual360())
        self.curve = PiecewiseYoYInflationCurve(
            TODAY,
            CURVE_BASE,
            YY_DATA[0][1] / 100.0,  # the base year-on-year rate at node zero
            Frequency.Monthly,  # what UK RPI publishes, not the annual leg's
            self.day_counter,
            self._helpers(),
        )
        # Until this link is made every forecast raises "empty Handle".
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
        """A year-on-year leg on the fixture schedule. The keywords go straight
        through: notional / notionals, gearing(s), spread(s), fixing_days and
        the payment adjustment."""
        return YoYInflationLeg(
            self.schedule,
            self.calendar,
            self.index,
            LAG,
            CpiInterpolationType.Flat,
            self.day_counter,
            **overrides,
        )

    def swap(self, spread: float = 0.0) -> YearOnYearInflationSwap:
        """A payer swap: SwapType names the FIXED leg, so a Payer pays the 3%
        fixed and receives inflation. Both legs run the same annual schedule
        here, but they are independent inputs."""
        swap = YearOnYearInflationSwap(
            SwapType.Payer,
            NOTIONAL,
            self.schedule,  # fixed schedule
            FIXED_RATE,
            self.day_counter,
            self.schedule,  # year-on-year schedule
            self.index,
            LAG,
            CpiInterpolationType.Flat,
            spread,
            self.day_counter,
            self.calendar,  # the year-on-year leg's payment calendar
            BusinessDayConvention.ModifiedFollowing,
            self.settings,
        )
        swap.set_engine(DiscountingSwapEngine(self.nominal, self.settings))
        return swap


def price_the_swap(market: Market) -> None:
    swap = market.swap()
    print(f"3Y year-on-year inflation swap, payer of {FIXED_RATE:.2%} on {NOTIONAL:,.0f}, to {MATURITY}:")
    print(f"  NPV                  = {swap.npv():14.4f}")
    print(f"  fixed leg NPV        = {swap.fixed_leg_npv():14.4f}")
    print(f"  year-on-year leg NPV = {swap.yoy_leg_npv():14.4f}")
    print(f"  fair fixed rate      = {swap.fair_rate() * 100:10.6f}%   (prices the swap at zero)")
    print(f"  fair spread          = {swap.fair_spread() * 100:10.6f}%   (over the index instead)")
    # A free self-check: the swap runs to the third quoted pillar, so its fair
    # rate must come back as that pillar's own quote. If it drifts, the curve
    # was bootstrapped off a fixture that does not match the schedule.
    print(f"  quoted {MATURITY} rate = {YY_DATA[2][1]:10.6f}%   (the fair rate must reproduce it)")


def inspect_the_coupons(market: Market) -> None:
    """The leg built standalone hands back coupons, and each one reports where
    its rate came from. The coupons are bound once: the leg rebuilds them on
    every `coupons()` call, so reading through two calls compares different
    objects."""
    gearing, spread = 1.5, 0.002
    coupons = market.leg(notional=NOTIONAL, gearing=gearing, spread=spread).coupons()

    print(f"\n\nStandalone leg, geared {gearing} with a {spread:.2%} spread ({len(coupons)} coupons):")
    for coupon in coupons:
        print(
            f"  {coupon.accrual_start_date()} -> {coupon.accrual_end_date()}  "
            f"pays {coupon.date()}  fixing={coupon.index_fixing():.8f}"
        )
        print(
            f"      rate = {gearing} * fixing + {spread} = {coupon.rate():.8f}   "
            f"amount = rate * {coupon.accrual_period():.4f} * {coupon.nominal():,.0f} = {coupon.amount():.4f}"
        )


def the_leg_is_the_swaps_leg(market: Market) -> None:
    """A plain leg, discounted coupon by coupon on the nominal curve, is the
    year-on-year leg of the swap built over the same schedule and index. The
    swap reaches its leg through the core builder this facade wraps, so the two
    agreeing pins the wiring: the notional broadcast, the payment roll and the
    discounting all have to line up.

    Discounting to the curve reference date is exact here because the engine's
    NPV date defaults to it, leaving nothing to divide out."""
    coupons = market.leg(notional=NOTIONAL).coupons()
    discounted = sum(coupon.amount() * market.nominal.discount_date(coupon.date(), True) for coupon in coupons)
    from_swap = market.swap().yoy_leg_npv()

    print("\n\nThe standalone leg is the swap's own leg:")
    print(f"  discounted coupons = {discounted:.8f}")
    print(f"  swap.yoy_leg_npv() = {from_swap:.8f}")
    print(f"  |diff|             = {abs(discounted - from_swap):.2e}")


def the_rate_lags_off_the_accrual_end(market: Market) -> None:
    """With no fixing days a coupon's fixing date and its lagged accrual end are
    the same day, so the two possible rules agree by accident. Fifteen fixing
    days roll the fixing date back into the previous month, where the curve
    carries a different rate - and the coupon stays on the accrual-end figure."""
    coupon = market.leg(notional=NOTIONAL, fixing_days=15).coupons()[2]
    accrual_end_month = Date(1, 6, 2010)
    fixing_date_month = Date(1, 5, 2010)

    print("\n\nThe rate lags off the accrual end, not off the fixing date:")
    print(f"  accrual ends       {coupon.accrual_end_date()}, less the {LAG} lag, snapped -> {accrual_end_month}")
    print(f"  fixing date        {coupon.fixing_date()} (15 days back), which snaps -> {fixing_date_month}")
    print(f"  index at {accrual_end_month} = {market.index.fixing(accrual_end_month):.8f}  <- what the coupon reads")
    print(f"  index at {fixing_date_month} = {market.index.fixing(fixing_date_month):.8f}  <- the other month")
    print(f"  coupon.index_fixing()      = {coupon.index_fixing():.8f}")


def main() -> None:
    market = Market()
    price_the_swap(market)
    inspect_the_coupons(market)
    the_leg_is_the_swaps_leg(market)
    the_rate_lags_off_the_accrual_end(market)


if __name__ == "__main__":
    main()
