"""Price credit default swaps under the ISDA standard model.

`credit_cds.py` next door prices a CDS with `MidPointCdsEngine`, which
integrates over the premium schedule. `IsdaCdsEngine` is the market-standard
alternative: it integrates both legs over the pillar dates of the two curves it
is built with, and it is what Markit's published upfronts are computed on. The
whole ISDA chain shows up here:

* a generic `IborIndex` on ISDA conventions (`Calendar.weekends_only`, Act/360,
  a named `Currency`) rather than a Euribor from the named families,
* `DepositRateHelper.from_rate` and `SwapRateHelper` feeding a
  `PiecewiseLogLinearDiscount` curve - log-linear on discount factors, Act/365F,
  which is the curve shape the ISDA model is specified against,
* `MakeCreditDefaultSwap`, the post-Big-Bang builder that derives the premium
  schedule from a term date and takes its trade date from the evaluation date,
* `implied_hazard_rate`, which inverts a quoted trade to the flat hazard rate
  that prices it at zero, standing on its own engine rather than on any curve
  passed in,
* `fair_upfront` and the accrual-rebate accessors.

Two parts.

1. The Markit upfront grid: five term dates crossed with two quoted spreads and
   two recovery rates, off a 21 May 2009 USD curve. Each case implies a hazard
   rate off a trade quoted at `spread`, then prices a *separate* 1% conventional
   trade of the same maturity on that rate. Pricing the quoted trade itself
   would be a different number entirely. Mirrors
   `crates/itofin-py/tests/test_isda_markit.py` and QuantLib's `testIsdaEngine`
   (`test-suite/creditdefaultswap.cpp:567`).

2. The EUR reconciliation pair: one record traded today, which rebates the
   thousand of accrual it has run up, and the same record traded two years
   earlier, which settled that rebate long before today. The value differs by
   exactly that thousand. Mirrors `test_isda_reconcile.py` and
   `creditdefaultswap.cpp:759-960`.

The Markit literals are Markit's, not a recording of what these facades return,
so the printed differences grade the whole chain. QuantLib compares them with
Boost's `BOOST_CHECK_CLOSE`, whose tolerance is a *percentage*: the 1e-6 there
is a relative 1e-8, which is why the differences below are reported relative
rather than absolute.

Run it with:

    python example/python/isda_cds.py
"""

# plugins
# itofin library
from itofin import Settings
from itofin.indexes import Currency, IborIndex
from itofin.instruments import MakeCreditDefaultSwap, PricingModel, ProtectionSide
from itofin.pricingengines import IsdaCdsEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import DepositRateHelper, FlatHazardRate, PiecewiseLogLinearDiscount, SwapRateHelper
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Period

ACT365F = DayCounter.actual365_fixed()
WEEKENDS = Calendar.weekends_only()


def isda_ibor(tenor: Period, currency: Currency, settings: Settings) -> IborIndex:
    """The ISDA-convention forecasting index, spelled out because it belongs to
    no named family. `forwarding=None` builds it over an empty handle: both
    helpers re-point their own clone at the curve being bootstrapped, so the
    index must not already carry one."""
    return IborIndex(
        "IsdaIbor",
        tenor,
        2,  # settlement days
        currency,
        WEEKENDS,
        BusinessDayConvention.ModifiedFollowing,
        False,  # end of month
        DayCounter.actual360(),
        None,  # forwarding curve
        settings,
    )


def isda_curve(reference, deposits, swaps, float_tenor, fixed_frequency, currency, settings):
    """The ISDA discount curve: deposit quotes in months, par swap quotes in
    years, bootstrapped log-linearly on discount factors over Act/365F. One
    index of `float_tenor` feeds every swap helper."""
    helpers = [
        DepositRateHelper.from_rate(quote, isda_ibor(Period(months, "Months"), currency, settings))
        for months, quote in deposits
    ]
    floating = isda_ibor(float_tenor, currency, settings)
    helpers += [
        SwapRateHelper(
            SimpleQuote(quote),
            Period(years, "Years"),
            WEEKENDS,
            fixed_frequency,
            BusinessDayConvention.ModifiedFollowing,
            DayCounter.thirty360_bond_basis(),
            floating,
        )
        for years, quote in swaps
    ]
    return PiecewiseLogLinearDiscount(reference, helpers, ACT365F)


# --- part 1: the Markit upfront grid -----------------------------------------

TRADE_DATE = Date(21, 5, 2009)
GRID_NOTIONAL = 10_000_000.0
CONVENTIONAL_SPREAD = 0.01

# creditdefaultswap.cpp:583-588, tenors in months.
USD_DEPOSITS = [
    (1, 0.003081),
    (2, 0.005525),
    (3, 0.007163),
    (6, 0.012413),
    (9, 0.014),
    (12, 0.015488),
]

# creditdefaultswap.cpp:598-611, tenors in years.
USD_SWAPS = [
    (2, 0.011907),
    (3, 0.01699),
    (4, 0.021198),
    (5, 0.02444),
    (6, 0.026937),
    (7, 0.028967),
    (8, 0.030504),
    (9, 0.031719),
    (10, 0.03279),
    (12, 0.034535),
    (15, 0.036217),
    (20, 0.036981),
    (25, 0.037246),
    (30, 0.037605),
]

TERM_DATES = [
    Date(20, 6, 2010),
    Date(20, 6, 2011),
    Date(20, 6, 2012),
    Date(20, 6, 2016),
    Date(20, 6, 2019),
]
SPREADS = [0.001, 0.1]
RECOVERIES = [0.2, 0.4]

# creditdefaultswap.cpp:643-664: Markit's ISDA-model upfronts on a ten-million
# notional, in the term-date / spread / recovery order the loops visit.
MARKIT_VALUES = [
    -97798.29358,
    -97776.11889,
    914971.5977,
    894985.6298,
    -186921.3594,
    -186839.8148,
    1646623.672,
    1579803.626,
    -274298.9203,
    -274122.4725,
    2279730.93,
    2147972.527,
    -592420.2297,
    -591571.2294,
    3993550.206,
    3545843.418,
    -797501.1422,
    -795915.9787,
    4702034.688,
    4042340.999,
]


def grid_cases():
    """The 20 (term date, spread, recovery) triples, in the loop order the
    Markit values are indexed by."""
    return [
        (term_date, spread, recovery) for term_date in TERM_DATES for spread in SPREADS for recovery in RECOVERIES
    ]


def markit_grid() -> None:
    settings = Settings()
    # The evaluation date goes in before any helper is built: the helpers date
    # off it, and MakeCreditDefaultSwap derives its trade date from it.
    settings.set_evaluation_date(TRADE_DATE)
    discount = isda_curve(
        TRADE_DATE,
        USD_DEPOSITS,
        USD_SWAPS,
        Period(3, "Months"),
        Frequency.Semiannual,
        Currency.usd(),
        settings,
    )

    def trade(term_date, running_spread):
        return MakeCreditDefaultSwap(term_date, running_spread, settings, nominal=GRID_NOTIONAL).build()

    print(f"ISDA discount curve off {TRADE_DATE}: {len(USD_DEPOSITS)} deposits + {len(USD_SWAPS)} swaps")
    print(f"  5y discount factor = {discount.discount(5.0):.10f}")

    print("\nMarkit upfront grid (spread 0.1%, recovery 40%, conventional 1% trade):")
    worst_relative = 0.0
    for index, (term_date, spread, recovery) in enumerate(grid_cases()):
        # Invert the quoted trade to the flat hazard rate that prices it at
        # zero. `day_counter` counts the flat curve the solve builds, not the
        # contract; under PricingModel.Isda it must be Act/365F, as must the
        # discount curve, because that is what the ISDA engine demands.
        hazard_rate = trade(term_date, spread).implied_hazard_rate(
            0.0,  # target NPV
            discount,
            ACT365F,
            recovery,
            1e-10,  # accuracy
            PricingModel.Isda,
        )
        curve = FlatHazardRate.moving_with_rate(0, WEEKENDS, hazard_rate, ACT365F, settings)

        conventional = trade(term_date, CONVENTIONAL_SPREAD)
        conventional.set_isda_engine(IsdaCdsEngine(curve, recovery, discount, settings))
        upfront = conventional.notional() * conventional.fair_upfront()

        expected = MARKIT_VALUES[index]
        relative = abs(upfront - expected) / abs(expected)
        worst_relative = max(worst_relative, relative)

        # One representative row per term date, at the tight spread and the
        # higher recovery; the other 15 cases still run and still feed the worst
        # relative error below.
        if spread == SPREADS[0] and recovery == RECOVERIES[1]:
            print(
                f"  {term_date}  hazard={hazard_rate:.8f}  upfront={upfront:15.4f}  "
                f"Markit={expected:14.4f}  rel={relative:.2e}"
            )

    print(f"\n  worst relative error over all {len(MARKIT_VALUES)} cases = {worst_relative:.2e}")
    print("  (QuantLib's own bound is a relative 1e-8, two-sided)")


# --- part 2: the EUR reconciliation pair --------------------------------------

VALUE_DATE = Date(26, 7, 2021)
MATURITY = Date(20, 6, 2026)
EUR_NOMINAL = 1_000_000.0
RECOVERY = 0.4
EUR_CONVENTIONAL_SPREAD = 0.006713
RUNNING_SPREAD = 0.01
PAST_TRADE_DATE = Date(20, 7, 2019)

# creditdefaultswap.cpp:770-771, tenors in months.
EUR_DEPOSITS = [
    (1, -0.0056),
    (3, -0.005440),
    (6, -0.005190),
    (12, -0.004930),
]

# creditdefaultswap.cpp:781-793, tenors in years.
EUR_SWAPS = [
    (2, -0.004820),
    (3, -0.004420),
    (4, -0.003990),
    (5, -0.003520),
    (6, -0.002970),
    (7, -0.002370),
    (8, -0.001760),
    (9, -0.001140),
    (10, -0.000540),
    (12, 0.000570),
    (15, 0.001880),
    (20, 0.002940),
    (30, 0.002820),
]

# creditdefaultswap.cpp:759-960: the value of each record, and the accrual the
# traded-today one rebates.
MARKIT_TODAY_VALUE = -16070.7
MARKIT_PAST_VALUE = -17070.77
MARKIT_ACCRUAL = 1000.0


def eur_reconciliation() -> None:
    settings = Settings()
    settings.set_evaluation_date(VALUE_DATE)
    discount = isda_curve(
        VALUE_DATE,
        EUR_DEPOSITS,
        EUR_SWAPS,
        Period(6, "Months"),
        Frequency.Annual,
        Currency.eur(),
        settings,
    )

    def trade(running_spread, trade_date=None):
        # trade_date overrides the evaluation date the trade is otherwise dated
        # off, which is how a contract traded in the past is built.
        return MakeCreditDefaultSwap(
            MATURITY,
            running_spread,
            settings,
            nominal=EUR_NOMINAL,
            trade_date=trade_date,
        ).build()

    hazard_rate = trade(EUR_CONVENTIONAL_SPREAD).implied_hazard_rate(
        0.0, discount, ACT365F, RECOVERY, 1e-10, PricingModel.Isda
    )
    curve = FlatHazardRate.moving_with_rate(0, WEEKENDS, hazard_rate, ACT365F, settings)
    engine = IsdaCdsEngine(curve, RECOVERY, discount, settings)

    print(f"\n\nEUR reconciliation, {MATURITY} maturity valued {VALUE_DATE}")
    print(f"  hazard rate implied off the {EUR_CONVENTIONAL_SPREAD} conventional quote = {hazard_rate:.10f}")

    today_record = trade(RUNNING_SPREAD)
    today_record.set_isda_engine(engine)
    npv = today_record.npv()
    # The C++ fixture's own discount to cash settlement: the ratio of the
    # upfront to the value, which the derived accrual then divides back out. It
    # is deliberately not an independently computed discount factor.
    df = today_record.notional() * today_record.fair_upfront() / npv
    derived_accrual = df * (npv - today_record.default_leg_npv() - today_record.coupon_leg_npv())

    print("\n  Traded today - the accrual rebate is still in the value:")
    print(
        f"    NPV                    = {npv:12.4f}   Markit={MARKIT_TODAY_VALUE}   "
        f"rel={abs(npv / MARKIT_TODAY_VALUE - 1):.2e}"
    )
    print(f"    accrual from the legs  = {derived_accrual:12.4f}   Markit={MARKIT_ACCRUAL}")
    print(f"    accrual_rebate_amount  = {today_record.accrual_rebate_amount():12.4f}")
    print(f"    accrual_rebate_date    = {today_record.accrual_rebate_date()}  (3 business days off the trade date)")

    past_record = trade(RUNNING_SPREAD, trade_date=PAST_TRADE_DATE)
    past_record.set_isda_engine(engine)
    past_npv = past_record.npv()
    # The contract still carries the rebate flow - the core builds one whenever
    # the flag is set, whatever the trade date - but it settled on a past date,
    # so it no longer reaches the value and the two legs close the arithmetic.
    residual = past_npv - past_record.default_leg_npv() - past_record.coupon_leg_npv()

    print(f"\n  Traded {PAST_TRADE_DATE} - the rebate settled long ago:")
    print(
        f"    NPV                    = {past_npv:12.4f}   Markit={MARKIT_PAST_VALUE}   "
        f"rel={abs(past_npv / MARKIT_PAST_VALUE - 1):.2e}"
    )
    print(f"    accrual from the legs  = {residual:12.4f}   (nothing left over)")
    print(f"    accrual_rebate_date    = {past_record.accrual_rebate_date()}  (before the value date)")
    # The two Markit literals are rounded to 2dp and differ by 1000.07, so the
    # rebate flow's own amount is the sharper comparison of the two.
    print(f"\n  value difference   = {npv - past_npv:.4f}")
    print(f"  rebate on the flow = {today_record.accrual_rebate_amount():.4f}")
    print(f"  the Markit pair differs by {MARKIT_TODAY_VALUE - MARKIT_PAST_VALUE:.4f} at their printed precision")


def main() -> None:
    markit_grid()
    eur_reconciliation()


if __name__ == "__main__":
    main()
