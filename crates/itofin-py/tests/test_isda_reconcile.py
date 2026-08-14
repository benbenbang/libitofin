"""The two EUR Markit reconciliation records, through the Python facades (#870).

The oracle is QuantLib's `testIsdaEngine` reconciliation half
(test-suite/creditdefaultswap.cpp:759-960), which the Rust core reproduces in
`a_traded_today_record_reconciles_with_its_accrual_rebate` and
`a_record_traded_in_the_past_reconciles_without_its_accrual_rebate`
(crates/libitofin/src/pricingengines/credit/isdacdsengine.rs:2085 and :2147).
Both fixtures are rebuilt here: the same 26 July 2021 value date, the same four
deposit and thirteen swap EUR quotes bootstrapped log-linearly on discount
factors over Act/365F, and the same flat hazard rate implied off a trade quoted
at the conventional 0.006713 spread.

One record is traded today and rebates the thousand of accrual it has run up;
the same record traded two years earlier settled that rebate long before today,
so the value drops by exactly that thousand and the two legs then account for
all of what is left. The MARKIT_VALUE literals are Markit's, so they grade the
whole chain rather than restating it.

The helpers below are the USD ones of test_isda_markit.py parameterized on what
the EUR curve changes - the quotes, the 6M float tenor, the Annual fixed
frequency and the currency. They are duplicated rather than imported because no
test module in this suite imports another and there is no conftest to share
them through.

The tolerance is Boost's `BOOST_CHECK_CLOSE`, a *percentage*, so the bound is a
relative 1e-5 and it is two-sided against both operands.
"""

from itofin import Settings
from itofin.indexes import Currency, IborIndex
from itofin.instruments import (
    CreditDefaultSwap,
    MakeCreditDefaultSwap,
    PricingModel,
    ProtectionSide,
)
from itofin.pricingengines import IsdaCdsEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    DepositRateHelper,
    FlatHazardRate,
    PiecewiseLogLinearDiscount,
    SwapRateHelper,
)
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    DayCounter,
    Frequency,
    Period,
    Schedule,
)

# creditdefaultswap.cpp:765 and :869: today, on both records.
VALUE_DATE = Date(26, 7, 2021)
MATURITY = Date(20, 6, 2026)

# creditdefaultswap.cpp:770-771 and :875-876, in months.
EUR_DEPOSITS = [
    (1, -0.0056),
    (3, -0.005440),
    (6, -0.005190),
    (12, -0.004930),
]

# creditdefaultswap.cpp:781-793 and :886-898, in years.
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

NOMINAL = 1000000.0
RECOVERY = 0.4
CONVENTIONAL_SPREAD = 0.006713
RUNNING_SPREAD = 0.01
CASH_SETTLEMENT_DAYS = 3

TOLERANCE = 1e-3
RELATIVE = TOLERANCE / 100.0


def assert_close(actual, expected, what):
    """Boost's BOOST_CHECK_CLOSE: a percentage tolerance, cleared against both
    operands. With a zero expected the bound collapses to bit-exact equality,
    which is the intended reading; only the diagnostic guards the division."""
    difference = abs(actual - expected)
    relative = difference / abs(expected) if expected else "undefined against 0"
    assert difference <= RELATIVE * abs(actual) and difference <= RELATIVE * abs(
        expected
    ), (
        f"{what}: {actual} is not within {TOLERANCE}% of {expected} "
        f"(relative {relative})"
    )


def ymd(date):
    """A date as a comparable triple: Date carries __eq__ but no ordering."""
    return (date.year, date.month, date.day)


def isda_ibor(tenor, currency, settings):
    """The ISDA-convention forecasting index (isdacdsengine.rs:1741-1758), over
    an empty forwarding handle because both helpers re-point their own clone at
    the curve being bootstrapped. Only the tenor and the currency vary."""
    return IborIndex(
        "IsdaIbor",
        tenor,
        2,
        currency,
        Calendar.weekends_only(),
        BusinessDayConvention.ModifiedFollowing,
        False,
        DayCounter.actual360(),
        None,
        settings,
    )


def isda_curve(
    reference, deposits, swaps, float_tenor, fixed_frequency, currency, settings
):
    """The ISDA discount curve (isdacdsengine.rs:1768-1806): deposits in months,
    swaps in years, bootstrapped log-linearly on discount factors over Act/365F.
    One index of `float_tenor` feeds every swap helper."""
    helpers = [
        DepositRateHelper.from_rate(
            quote, isda_ibor(Period(months, "Months"), currency, settings)
        )
        for months, quote in deposits
    ]
    floating = isda_ibor(float_tenor, currency, settings)
    helpers += [
        SwapRateHelper(
            SimpleQuote(quote),
            Period(years, "Years"),
            Calendar.weekends_only(),
            fixed_frequency,
            BusinessDayConvention.ModifiedFollowing,
            DayCounter.thirty360_bond_basis(),
            floating,
        )
        for years, quote in swaps
    ]
    return PiecewiseLogLinearDiscount(reference, helpers, DayCounter.actual365_fixed())


def cash_settlement(trade_date):
    """The date the upfront and the rebate settle on: three business days off
    the trade date, rolled Following on WeekendsOnly (makecds.rs:295-303)."""
    return Calendar.weekends_only().advance(
        trade_date,
        CASH_SETTLEMENT_DAYS,
        "Days",
        BusinessDayConvention.Following,
        False,
    )


class Market:
    """The EUR curve of both records and the engine over the flat hazard rate
    implied off a trade quoted at the conventional spread
    (isdacdsengine.rs:2042-2076).

    The evaluation date is set before any helper is built: the helpers date off
    it and MakeCreditDefaultSwap derives its trade date from it.
    """

    def __init__(self):
        self.settings = Settings()
        self.settings.set_evaluation_date(VALUE_DATE)
        self.discount = isda_curve(
            VALUE_DATE,
            EUR_DEPOSITS,
            EUR_SWAPS,
            Period(6, "Months"),
            Frequency.Annual,
            Currency.eur(),
            self.settings,
        )

    def trade(self, running_spread, trade_date=None):
        return MakeCreditDefaultSwap(
            MATURITY,
            running_spread,
            self.settings,
            nominal=NOMINAL,
            trade_date=trade_date,
        ).build()

    def engine(self):
        hazard_rate = self.trade(CONVENTIONAL_SPREAD).implied_hazard_rate(
            0.0,
            self.discount,
            DayCounter.actual365_fixed(),
            RECOVERY,
            1e-10,
            PricingModel.Isda,
        )
        curve = FlatHazardRate.moving_with_rate(
            0,
            Calendar.weekends_only(),
            hazard_rate,
            DayCounter.actual365_fixed(),
            self.settings,
        )
        return IsdaCdsEngine(curve, RECOVERY, self.discount, self.settings)


def test_a_traded_today_record_reconciles_with_its_accrual_rebate():
    """creditdefaultswap.cpp:759-861: the record's value, its upfront and the
    thousand of accrual it rebates, read both off the legs and off the rebate
    flow itself.

    `df` is the C++ fixture's own: the ratio of the upfront to the value, which
    carries the discount to cash settlement and which the second and third
    assertions then divide back out. It is deliberately not an independently
    derived discount factor.
    """
    market = Market()
    conventional = market.trade(RUNNING_SPREAD)
    conventional.set_isda_engine(market.engine())

    npv = conventional.npv()
    calculated_upfront = conventional.notional() * conventional.fair_upfront()
    df = calculated_upfront / npv
    derived_accrual = df * (
        npv - conventional.default_leg_npv() - conventional.coupon_leg_npv()
    )

    assert_close(npv, -16070.7, "the value")
    assert_close(calculated_upfront, df * -16070.7, "the upfront")
    assert_close(derived_accrual, 1000.0, "the accrual derived from the legs")
    assert_close(
        conventional.accrual_rebate_amount(), 1000.0, "the accrual on the rebate flow"
    )
    assert conventional.accrual_rebate_date() == cash_settlement(VALUE_DATE)


def test_a_record_traded_in_the_past_reconciles_without_its_accrual_rebate():
    """creditdefaultswap.cpp:863-960: the same record traded two years ago
    settled its rebate long before today, so the value drops by exactly the
    thousand the rebate carried and the legs account for all of what is left.

    The contract still carries the rebate flow - the core builds one whenever
    the flag is set, whatever the trade date (creditdefaultswap.rs:515-544) -
    but it settled on a past date, which is why it no longer reaches the value
    and why the legs alone close the arithmetic to zero.
    """
    past_trade_date = Date(20, 7, 2019)
    market = Market()
    conventional = market.trade(RUNNING_SPREAD, trade_date=past_trade_date)
    conventional.set_isda_engine(market.engine())

    npv = conventional.npv()
    calculated_accrual = (
        npv - conventional.default_leg_npv() - conventional.coupon_leg_npv()
    )

    assert_close(npv, -17070.77, "the value")
    assert_close(calculated_accrual, 0.0, "the accrual derived from the legs")

    assert conventional.accrual_rebate_amount() is not None
    assert conventional.accrual_rebate_date() == cash_settlement(past_trade_date)
    assert ymd(conventional.accrual_rebate_date()) < ymd(VALUE_DATE)


def test_a_contract_that_does_not_rebate_accrual_carries_no_rebate_flow():
    """The only None arm the accessors have (creditdefaultswap.rs:1349): the
    flag, not a stale trade date. MakeCreditDefaultSwap does not expose it, so
    the pin goes through the with_terms constructor that does.

    Both arms are built on the one constructor, so a with_terms that never
    reached the rebate at all would fail the rebating half rather than pass
    both.
    """
    settings = Settings()
    settings.set_evaluation_date(VALUE_DATE)
    schedule = Schedule(
        VALUE_DATE,
        MATURITY,
        Frequency.Quarterly,
        Calendar.weekends_only(),
        BusinessDayConvention.Following,
    )

    def contract(**terms):
        return CreditDefaultSwap.with_terms(
            ProtectionSide.Buyer,
            NOMINAL,
            RUNNING_SPREAD,
            schedule,
            BusinessDayConvention.Following,
            DayCounter.actual360(),
            settings,
            **terms,
        )

    bare = contract(rebates_accrual=False)
    assert bare.accrual_rebate_amount() is None
    assert bare.accrual_rebate_date() is None

    rebating = contract()
    assert rebating.accrual_rebate_amount() is not None
    assert rebating.accrual_rebate_date() is not None
