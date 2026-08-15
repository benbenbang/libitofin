"""Price a credit default swap and bootstrap a default-probability curve.

Two parts:

1. Price a single CDS with `MidPointCdsEngine` off a flat hazard-rate curve.
   The engine needs a survival (hazard) curve, a recovery rate and a discount
   curve. This reproduces QuantLib's `creditdefaultswap.cpp` cached value:
   NPV 295.0153398 and fair spread 0.007517539081. Both are matched to 1e-7,
   the tolerance `test_credit_cds.py` grades them at: the fair spread's
   difference sits in the eighth decimal place, not in all twelve printed.

2. Bootstrap a `PiecewiseDefaultCurve` from `SpreadCdsHelper` quotes (the
   inverse problem: given market spreads, solve for the hazard curve), then
   rebuild each pillar's contract on that curve and confirm it reprices to its
   own input spread. Mirrors `defaultprobabilitycurves.cpp`.

The bootstrap round-trip is exact only when the rebuilt contract matches the
helper's own conventions to the day, so the schedule construction below is
copied verbatim from `test_credit_bootstrap.py` (rolled protection start,
unadjusted maturity, explicit `protection_start`). A "simplified" schedule still
runs but quietly misses the quotes.

Run it with:

    python example/python/credit_cds.py
"""

# itofin library
from itofin import Settings
from itofin.instruments import CreditDefaultSwap, ProtectionSide
from itofin.pricingengines import MidPointCdsEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward, FlatHazardRate, PiecewiseDefaultCurve, SpreadCdsHelper
from itofin.time import BusinessDayConvention, Calendar, Date, DateGeneration, DayCounter, Frequency, Period, Schedule

# The `creditdefaultswap.cpp` cached figures, as transcribed in
# `crates/itofin-py/tests/test_credit_cds.py`, which grades both at 1e-7.
CACHED_NPV = 295.0153398
CACHED_FAIR_SPREAD = 0.007517539081


def price_single_cds() -> None:
    """A 10Y CDS on a flat 1.234% hazard rate, discounted at a flat 6%."""
    today = Date(9, 6, 2006)
    settings = Settings()
    settings.set_evaluation_date(today)
    calendar = Calendar.target()
    day_counter = DayCounter.actual360()

    hazard = FlatHazardRate.moving(0, calendar, SimpleQuote(0.01234), day_counter, settings)
    discount = FlatForward(today, 0.06, day_counter)

    schedule = Schedule(
        Date(9, 6, 2005),  # issue
        Date(9, 6, 2015),  # maturity
        Frequency.Semiannual,
        calendar,
        BusinessDayConvention.ModifiedFollowing,
    )

    # `with_terms` is the self-documenting constructor: protection side,
    # notional, running spread, schedule, then named conventions. Its defaults
    # reproduce the cached fixture.
    cds = CreditDefaultSwap.with_terms(
        ProtectionSide.Seller,
        10000.0,  # notional
        0.0120,  # running spread
        schedule,
        BusinessDayConvention.ModifiedFollowing,
        day_counter,
        settings,
    )
    cds.set_engine(MidPointCdsEngine(hazard, 0.4, discount, settings))

    npv = cds.npv()
    fair = cds.fair_spread()
    print("10Y CDS (seller, notional 10000, spread 120bp, hazard 1.234%):")
    print(f"  NPV            = {npv:.7f}      cached={CACHED_NPV}   |diff|={abs(npv - CACHED_NPV):.2e}")
    print(f"  fair spread    = {fair:.12f}   cached={CACHED_FAIR_SPREAD}   |diff|={abs(fair - CACHED_FAIR_SPREAD):.2e}")
    print(f"  coupon leg NPV = {cds.coupon_leg_npv():.7f}")
    print(f"  default leg NPV= {cds.default_leg_npv():.7f}")


# --- bootstrap arm -----------------------------------------------------------

TODAY = Date(9, 6, 2006)
QUOTES = [0.005, 0.006, 0.007, 0.009]
TENORS = [1, 2, 3, 5]
RECOVERY_RATE = 0.4
SETTLEMENT_DAYS = 1


def _round_trip_schedule(calendar: Calendar, tenor: int) -> Schedule:
    """The rebuilt contract's schedule, matching the helper's own conventions:
    it starts at the rolled protection start and its maturity is left
    unadjusted. Copied verbatim from the bootstrap oracle."""
    start = calendar.adjust(TODAY + SETTLEMENT_DAYS, BusinessDayConvention.Following)
    end = calendar.advance(TODAY, tenor, "Years", BusinessDayConvention.Unadjusted, False)
    return Schedule(
        start,
        end,
        Frequency.Quarterly,
        calendar,
        BusinessDayConvention.Following,
        DateGeneration.TwentiethIMM,
        termination_convention=BusinessDayConvention.Unadjusted,
    )


def bootstrap_default_curve() -> None:
    """Solve a hazard curve from four CDS spreads, then reprice each contract
    off it and confirm it returns its own input spread."""
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    # The bootstrap is lazy; this flag must be set before the first curve read.
    settings.set_include_todays_cash_flows(True)
    calendar = Calendar.target()
    day_counter = DayCounter.thirty360_bond_basis()
    discount = FlatForward(TODAY, 0.06, DayCounter.actual360())

    helpers = [
        SpreadCdsHelper(
            SimpleQuote(quote),
            Period(tenor, "Years"),
            SETTLEMENT_DAYS,
            calendar,
            Frequency.Quarterly,
            BusinessDayConvention.Following,
            DateGeneration.TwentiethIMM,
            day_counter,
            RECOVERY_RATE,
            discount,
            settings,
        )
        for quote, tenor in zip(QUOTES, TENORS)
    ]

    curve = PiecewiseDefaultCurve(TODAY, helpers, day_counter)

    print("\nBootstrapped default curve (survival probabilities):")
    for date in curve.dates():
        sp = curve.survival_probability_date(date)
        print(f"  {date}   survival={sp:.6f}")

    print("\nReprice check (rebuilt contract vs input spread):")
    for quote, tenor in zip(QUOTES, TENORS):
        swap = CreditDefaultSwap.with_terms(
            ProtectionSide.Buyer,
            1.0,
            quote,
            _round_trip_schedule(calendar, tenor),
            BusinessDayConvention.Following,
            day_counter,
            settings,
            protection_start=TODAY + SETTLEMENT_DAYS,
        )
        swap.set_engine(MidPointCdsEngine(curve, RECOVERY_RATE, discount, settings))
        fair = swap.fair_spread()
        print(f"  {tenor}Y  fair={fair:.10f}  input={quote:.10f}  |diff|={abs(fair - quote):.2e}")


def main() -> None:
    price_single_cds()
    bootstrap_default_curve()


if __name__ == "__main__":
    main()
