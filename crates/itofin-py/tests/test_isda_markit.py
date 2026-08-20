"""The 20-case Markit ISDA upfront grid, through the Python facades (#816).

The oracle is QuantLib's own `testIsdaEngine` (test-suite/creditdefaultswap.cpp:567),
whose cached Markit upfronts the Rust core already reproduces in
`the_markit_grid_reproduces_the_isda_upfronts`
(crates/libitofin/src/pricingengines/credit/isdacdsengine.rs:1918). That fixture
is rebuilt here byte-for-byte: the same 21 May 2009 trade date, the same six
deposit and fourteen swap quotes bootstrapped log-linearly on discount factors
over Act/365F, and the same five term dates crossed with two spreads and two
recovery rates.

The literals below are Markit's, not a recording of what these facades return,
so they grade the whole chain - the generic IborIndex, both rate helpers, the
piecewise curve, the CDS builder, the implied-hazard solve and the ISDA engine -
rather than restating it.

Each case implies a flat hazard rate off a trade quoted at `spread` and then
prices a *separate* 1% conventional trade of the same maturity on that rate,
which is what the C++ fixture does; pricing the quoted trade itself would be a
different number entirely.

The tolerance is Boost's `BOOST_CHECK_CLOSE`, which the C++ fixture reaches
through `QL_CHECK_CLOSE`: its 1e-6 is a *percentage*, so the bound is a relative
1e-8, and it is two-sided - the difference must clear the bound against both
operands. `pytest.approx` is neither, hence the helper below.
"""

# pypi/conda library
import pytest

# itofin library
from itofin import ItofinError, Settings
from itofin.indexes import Currency, IborIndex
from itofin.instruments import MakeCreditDefaultSwap, PricingModel, ProtectionSide
from itofin.pricingengines import IsdaCdsEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import DepositRateHelper, FlatHazardRate, PiecewiseLogLinearDiscount, SwapRateHelper
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Period

TRADE_DATE = Date(21, 5, 2009)
NOTIONAL = 10000000.0
CONVENTIONAL_SPREAD = 0.01

# creditdefaultswap.cpp:583-588, in months.
USD_DEPOSITS = [
    (1, 0.003081),
    (2, 0.005525),
    (3, 0.007163),
    (6, 0.012413),
    (9, 0.014),
    (12, 0.015488),
]

# creditdefaultswap.cpp:598-611, in years.
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

# creditdefaultswap.cpp:643-664: the ISDA-model upfronts on a ten-million
# notional, in the term-date / spread / recovery order the loop visits.
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

TERM_DATES = [
    Date(20, 6, 2010),
    Date(20, 6, 2011),
    Date(20, 6, 2012),
    Date(20, 6, 2016),
    Date(20, 6, 2019),
]
SPREADS = [0.001, 0.1]
RECOVERIES = [0.2, 0.4]

TOLERANCE = 1e-6
RELATIVE = TOLERANCE / 100.0


def assert_close(actual, expected, what):
    """Boost's BOOST_CHECK_CLOSE: a percentage tolerance, cleared against both
    operands."""
    difference = abs(actual - expected)
    assert difference <= RELATIVE * abs(actual) and difference <= RELATIVE * abs(
        expected
    ), (
        f"{what}: {actual} is not within {TOLERANCE}% of {expected} "
        f"(relative {difference / abs(expected)})"
    )


def isda_ibor(tenor, settings):
    """The ISDA-convention forecasting index the C++ fixture builds inline
    (creditdefaultswap.cpp:616-618), over an empty forwarding handle because
    both helpers re-point their own clone at the curve being bootstrapped."""
    return IborIndex(
        "IsdaIbor",
        tenor,
        2,
        Currency.usd(),
        Calendar.weekends_only(),
        BusinessDayConvention.ModifiedFollowing,
        False,
        DayCounter.actual360(),
        None,
        settings,
    )


def isda_curve(settings):
    """The ISDA discount curve: deposits in months, swaps in years, bootstrapped
    log-linearly on discount factors over Act/365F
    (creditdefaultswap.cpp:628-632). One 3M index feeds every swap helper, as
    the C++ fixture and the Rust oracle both do."""
    helpers = [
        DepositRateHelper.from_rate(
            quote, isda_ibor(Period(months, "Months"), settings)
        )
        for months, quote in USD_DEPOSITS
    ]
    floating = isda_ibor(Period(3, "Months"), settings)
    helpers += [
        SwapRateHelper(
            SimpleQuote(quote),
            Period(years, "Years"),
            Calendar.weekends_only(),
            Frequency.Semiannual,
            BusinessDayConvention.ModifiedFollowing,
            DayCounter.thirty360_bond_basis(),
            floating,
        )
        for years, quote in USD_SWAPS
    ]
    return PiecewiseLogLinearDiscount(TRADE_DATE, helpers, DayCounter.actual365_fixed())


def cases():
    """The 20 (term date, spread, recovery) triples, in the loop order the
    Markit values are indexed by."""
    return [
        (term_date, spread, recovery)
        for term_date in TERM_DATES
        for spread in SPREADS
        for recovery in RECOVERIES
    ]


class Market:
    """The fixture's evaluation date and bootstrapped discount curve.

    The evaluation date is set before any helper is built: the helpers date off
    it and MakeCreditDefaultSwap derives its trade date from it.
    """

    def __init__(self):
        self.settings = Settings()
        self.settings.set_evaluation_date(TRADE_DATE)
        self.discount = isda_curve(self.settings)

    def trade(self, term_date, running_spread, upfront_rate=None, side=None):
        return MakeCreditDefaultSwap(
            term_date,
            running_spread,
            self.settings,
            nominal=NOTIONAL,
            upfront_rate=upfront_rate,
            side=side,
        ).build()

    def implied_hazard_rate(self, term_date, spread, recovery):
        return self.trade(term_date, spread).implied_hazard_rate(
            0.0,
            self.discount,
            DayCounter.actual365_fixed(),
            recovery,
            1e-10,
            PricingModel.Isda,
        )

    def engine(self, hazard_rate, recovery):
        curve = FlatHazardRate.moving_with_rate(
            0,
            Calendar.weekends_only(),
            hazard_rate,
            DayCounter.actual365_fixed(),
            self.settings,
        )
        return IsdaCdsEngine(curve, recovery, self.discount, self.settings)


def test_the_markit_grid_reproduces_the_isda_upfronts():
    market = Market()

    for index, (term_date, spread, recovery) in enumerate(cases()):
        hazard_rate = market.implied_hazard_rate(term_date, spread, recovery)
        conventional = market.trade(term_date, CONVENTIONAL_SPREAD)
        conventional.set_isda_engine(market.engine(hazard_rate, recovery))

        assert_close(
            conventional.notional() * conventional.fair_upfront(),
            MARKIT_VALUES[index],
            f"case {index} ({term_date}, spread {spread}, recovery {recovery})",
        )


def test_both_sides_are_worth_nothing_at_their_own_fair_upfront():
    """The second half of the C++ fixture (creditdefaultswap.cpp:690-721): once
    the fair upfront is paid, the trade is worth nothing to either side. The
    bound is absolute, on a ten-million notional."""
    market = Market()

    for index, (term_date, spread, recovery) in enumerate(cases()):
        hazard_rate = market.implied_hazard_rate(term_date, spread, recovery)
        engine = market.engine(hazard_rate, recovery)
        conventional = market.trade(term_date, CONVENTIONAL_SPREAD)
        conventional.set_isda_engine(engine)
        fair_upfront = conventional.fair_upfront()

        for side in [ProtectionSide.Buyer, ProtectionSide.Seller]:
            at_fair = market.trade(
                term_date,
                CONVENTIONAL_SPREAD,
                upfront_rate=fair_upfront,
                side=side,
            )
            at_fair.set_isda_engine(engine)

            npv = at_fair.npv()
            assert abs(npv) <= TOLERANCE, (
                f"case {index} {side} is worth {npv} at its own fair upfront"
            )


def test_the_two_sides_are_opposite_trades():
    """The reprice-to-zero pin above cannot see `side`: a Buyer built where a
    Seller was asked for is worth nothing at the fair upfront just the same. So
    the flag is pinned here instead, away from its own fair upfront, where the
    two sides are worth the exact negatives of each other."""
    market = Market()
    term_date = TERM_DATES[3]
    recovery = RECOVERIES[1]
    engine = market.engine(
        market.implied_hazard_rate(term_date, SPREADS[0], recovery), recovery
    )

    sides = []
    for side in [ProtectionSide.Buyer, ProtectionSide.Seller]:
        trade = market.trade(term_date, CONVENTIONAL_SPREAD, side=side)
        trade.set_isda_engine(engine)
        sides.append(trade.npv())

    assert abs(sides[0]) > 1.0
    assert sides[1] == -sides[0]


def test_a_builder_without_an_evaluation_date_is_refused():
    """The trade date is derived from the evaluation date (makecds.rs:286-294),
    so an unset one is an error rather than a fallback to a system clock (D10)."""
    settings = Settings()
    with pytest.raises(ItofinError):
        MakeCreditDefaultSwap(TERM_DATES[0], CONVENTIONAL_SPREAD, settings).build()
