"""Bootstrap a zero-coupon inflation curve and price inflation swaps.

A `ZeroCouponInflationSwap` exchanges one fixed flow for one inflation-indexed
flow at maturity. Pricing it needs three things wired together:

* a `ZeroInflationIndex` (here UK RPI) loaded with its published fixings,
* a zero-inflation term structure the index forecasts future fixings off,
* a nominal discount curve, via a `DiscountingSwapEngine`.

The index reaches the curve through `index.link_to(curve)`. Until that link is
made, any forecast raises "empty Handle", so it must happen before the swap
prices.

This example bootstraps a `PiecewiseZeroInflationCurve` from fourteen quoted
swap rates, then rebuilds each quoted swap standalone on a 5% nominal curve and
confirms it reprices to (essentially) zero. This mirrors QuantLib's
`inflation.cpp` `testZeroTermStructure`.

The evaluation date is 13 August 2007 deliberately: it matches QuantLib's
fixture, whose RPI history runs from January 2005. All 31 fixings are kept
because `index.last_fixing_date()` (July 2007) becomes the curve's base date;
trimming the table would silently move it.

Run it with:

    python example/python/inflation_swap.py
"""

# plugins
# itofin library
from itofin import Settings
from itofin.indexes import CpiInterpolationType, ZeroInflationIndex
from itofin.instruments import SwapType, ZeroCouponInflationSwap
from itofin.pricingengines import DiscountingSwapEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    FlatForward,
    MultiplicativePriceSeasonality,
    PiecewiseZeroInflationCurve,
    ZeroCouponInflationSwapHelper,
)
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Period

TODAY = Date(13, 8, 2007)
LAG = Period(3, "Months")
NOMINAL = 1_000_000.0
NOMINAL_RATE = 0.05

# 31 monthly UK RPI figures, published from January 2005 (index level, not a
# rate). The last, 207.3, is the July 2007 period that forecasts compound off.
FIX_DATA = [
    189.9,
    189.9,
    189.6,
    190.5,
    191.6,
    192.0,
    192.2,
    192.2,
    192.6,
    193.1,
    193.3,
    193.6,
    194.1,
    193.4,
    194.2,
    195.0,
    196.5,
    197.7,
    198.5,
    198.5,
    199.2,
    200.1,
    200.4,
    201.1,
    202.7,
    201.6,
    203.1,
    204.4,
    205.4,
    206.2,
    207.3,
]

# (maturity, quoted zero-coupon inflation swap rate in percent).
ZC_DATA = [
    (Date(13, 8, 2008), 2.93),
    (Date(13, 8, 2009), 2.95),
    (Date(13, 8, 2010), 2.965),
    (Date(15, 8, 2011), 2.98),
    (Date(13, 8, 2012), 3.0),
    (Date(13, 8, 2014), 3.06),
    (Date(13, 8, 2017), 3.175),
    (Date(13, 8, 2019), 3.243),
    (Date(15, 8, 2022), 3.293),
    (Date(14, 8, 2027), 3.338),
    (Date(13, 8, 2032), 3.348),
    (Date(15, 8, 2037), 3.348),
    (Date(13, 8, 2047), 3.308),
    (Date(13, 8, 2057), 3.228),
]


# Twelve monthly seasonal factors (`inflation.cpp:469-483`), anchored on the 31
# January of the year the curve's base period ends in. They are stationary, so
# the lookup wrapping modulo twelve reuses them for every later year.
SEASONALITY_FACTORS = [
    1.003245, 1.000000, 0.999715, 1.000495, 1.000929, 0.998687,
    0.995949, 0.994682, 0.995949, 1.000519, 1.003705, 1.004186,
]
SEASONALITY_ANCHOR = Date(31, 1, 2007)
SEASONALITY_PROBE = Date(1, 8, 2012)


def _fixing_date(i: int) -> Date:
    """The i-th monthly fixing date, walked from 1 January 2005. A monthly index
    files each figure under the first of its month."""
    return Date(1, i % 12 + 1, 2005 + i // 12)


def build_index(settings: Settings) -> ZeroInflationIndex:
    index = ZeroInflationIndex.uk_rpi(settings)
    for i, fixing in enumerate(FIX_DATA):
        index.add_fixing(_fixing_date(i), fixing)
    return index


def nominal_curve() -> FlatForward:
    """The 5% discount curve the standalone swaps price on."""
    return FlatForward(TODAY, NOMINAL_RATE, DayCounter.actual360())


def build_helpers(settings, index):
    return [
        ZeroCouponInflationSwapHelper(
            SimpleQuote(rate / 100.0),
            LAG,
            maturity,
            Calendar.united_kingdom(),
            BusinessDayConvention.ModifiedFollowing,
            DayCounter.thirty360_bond_basis(),
            index,
            CpiInterpolationType.Flat,
            settings,
        )
        for maturity, rate in ZC_DATA
    ]


def build_swap(settings, index, maturity, fixed_rate):
    """A payer zero-coupon inflation swap: pays inflation, receives fixed.

    Positional constructor (see instruments.pyi): swap type, nominal, start,
    maturity, fixed calendar, fixed convention, day count, fixed rate, index,
    observation lag, observation interpolation, then optional inflation-leg
    calendar and convention (None -> fall back to the fixed-leg ones)."""
    swap = ZeroCouponInflationSwap(
        SwapType.Payer,
        NOMINAL,
        TODAY,
        maturity,
        Calendar.united_kingdom(),
        BusinessDayConvention.ModifiedFollowing,
        DayCounter.thirty360_bond_basis(),
        fixed_rate,
        index,
        LAG,
        CpiInterpolationType.Flat,
        None,  # inflation calendar -> fixed-leg calendar
        None,  # inflation convention -> fixed-leg convention
        settings,
    )
    swap.set_engine(DiscountingSwapEngine(nominal_curve(), settings))
    return swap


def apply_seasonality(settings, index, curve) -> None:
    """Install a seasonal correction on the solved curve and watch it move.

    A price index does not grow smoothly through the year, so
    `MultiplicativePriceSeasonality` multiplies the index level by a factor per
    period. The factors are not applied raw: the one at the queried date is
    normalized against the one at the curve's base date, so the correction is
    the identity there and grows away from it.

    Two things worth seeing. The correction reaches everything through the
    index's own forecast, which is the path every helper and every swap takes;
    and because the curve re-solves against it, the quoted swaps still reprice
    to zero afterwards. A correction that were merely stored and never folded
    into the published rate would also reprice to zero, so the forecast moving
    is the part that proves it is live."""
    raw_forecast = index.fixing(SEASONALITY_PROBE, True)
    raw_rate = curve.zero_rate_date(SEASONALITY_PROBE)

    curve.set_seasonality(MultiplicativePriceSeasonality(SEASONALITY_ANCHOR, Frequency.Monthly, SEASONALITY_FACTORS))

    print(f"\nSeasonality installed (has_seasonality={curve.has_seasonality()}), probed at {SEASONALITY_PROBE}:")
    print(f"  index forecast  {raw_forecast:.10f} -> {index.fixing(SEASONALITY_PROBE, True):.10f}")
    print(f"  zero rate       {raw_rate * 100:.6f}% -> {curve.zero_rate_date(SEASONALITY_PROBE) * 100:.6f}%")

    worst = max(abs(build_swap(settings, index, maturity, rate / 100.0).npv()) for maturity, rate in ZC_DATA)
    print(f"  worst |NPV| after the re-bootstrap = {worst:.3e}   (the quotes still reprice to zero)")

    # Clearing it puts the raw forecast back, which is how it is undone.
    curve.set_seasonality(None)
    print(
        f"  cleared (has_seasonality={curve.has_seasonality()}): "
        f"forecast back to {index.fixing(SEASONALITY_PROBE, True):.10f}"
    )


def main() -> None:
    # D5: one Settings threaded through index, helpers, swaps and engines.
    settings = Settings()
    settings.set_evaluation_date(TODAY)

    index = build_index(settings)
    print(f"UK RPI last fixing date (curve base) = {index.last_fixing_date()}")

    # Bootstrap the zero-inflation curve from the fourteen quoted swap rates.
    curve = PiecewiseZeroInflationCurve(
        TODAY,
        index.last_fixing_date(),
        Frequency.Monthly,
        DayCounter.thirty360_bond_basis(),
        build_helpers(settings, index),
    )
    # Link the index to the solved curve so standalone swaps can forecast off it.
    index.link_to(curve)

    # Price one swap in detail.
    maturity, rate = ZC_DATA[4]  # the 5Y-ish 2012 pillar
    swap = build_swap(settings, index, maturity, rate / 100.0)
    print(f"\nZero-coupon inflation swap to {maturity} (quote {rate}%):")
    print(f"  fair rate        = {swap.fair_rate() * 100:.6f}%")
    print(f"  NPV              = {swap.npv():.4f}")
    print(f"  fixed leg NPV    = {swap.fixed_leg_npv():.4f}")
    print(f"  inflation leg NPV= {swap.inflation_leg_npv():.4f}")

    # The milestone: every quoted swap, rebuilt standalone and discounted on the
    # 5% nominal curve, reprices to essentially nothing off the bootstrapped
    # inflation curve.
    print("\nReprice check (quoted swaps off the bootstrapped curve):")
    worst = 0.0
    for maturity, rate in ZC_DATA:
        npv = build_swap(settings, index, maturity, rate / 100.0).npv()
        worst = max(worst, abs(npv))
        print(f"  {maturity}  NPV={npv:14.6f}")
    print(f"  worst |NPV| = {worst:.3e}  (should be ~1e-8 or smaller)")

    apply_seasonality(settings, index, curve)


if __name__ == "__main__":
    main()
