"""Bootstrap round-trip oracle for the piecewise credit curve (#742).

Mirrors the core fixture at ``piecewisedefaultcurve.rs:436-525``, itself
``defaultprobabilitycurves.cpp`` ``testBootstrapFromSpread<HazardRate,
BackwardFlat>`` (``:152-224``): four CDS quotes are bootstrapped into a hazard
curve, then each pillar's contract is rebuilt from the market conventions and
repriced off that curve, and must return its own input spread.

The round trip is non-tautological because the contract is built fresh rather
than asked back from the helper: a facade that dropped the quote, the rule or
the protection start would still produce a curve, but not one the rebuilt
contract reprices on. ``fair_spread`` is compared, not ``implied_quote``.

Three things the milestone depends on and that a looser fixture would hide:

* ``include_todays_cash_flows`` is set before the first read, not merely before
  the constructor - the curve is lazy, so the first read is what runs the
  bootstrap the flag governs.
* the round-trip schedule starts at the rolled protection start
  (``piecewisedefaultcurve.rs:403-405``), as the helper's own does
  (``defaultprobabilityhelpers.rs:496-498``). Starting it at the evaluation date
  instead adds three days of premium accrual against an unchanged protection
  leg.
* ``protection_start`` is passed explicitly. Left out, it defaults to the
  schedule's first date, which the twentieth-IMM rule puts ten days later.
"""

import pytest
from itofin import ItofinError, Settings
from itofin.instruments import CreditDefaultSwap, ProtectionSide
from itofin.pricingengines import MidPointCdsEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward, PiecewiseDefaultCurve, SpreadCdsHelper
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

TODAY = Date(9, 6, 2006)
QUOTES = [0.005, 0.006, 0.007, 0.009]
TENORS = [1, 2, 3, 5]
RECOVERY_RATE = 0.4
SETTLEMENT_DAYS = 1
TOLERANCE = 1.0e-6


class Market:
    """The settings, discount curve and helpers the bootstrap runs on."""

    def __init__(self):
        self.settings = Settings()
        self.settings.set_evaluation_date(TODAY)
        self.settings.set_include_todays_cash_flows(True)
        self.calendar = Calendar.target()
        self.day_counter = DayCounter.thirty360_bond_basis()
        self.discount = FlatForward(TODAY, 0.06, DayCounter.actual360())
        self.helpers = [
            SpreadCdsHelper(
                SimpleQuote(quote),
                Period(tenor, "Years"),
                SETTLEMENT_DAYS,
                self.calendar,
                Frequency.Quarterly,
                BusinessDayConvention.Following,
                DateGeneration.TwentiethIMM,
                self.day_counter,
                RECOVERY_RATE,
                self.discount,
                self.settings,
            )
            for quote, tenor in zip(QUOTES, TENORS)
        ]
        self.curve = PiecewiseDefaultCurve(TODAY, self.helpers, self.day_counter)

    def round_trip_schedule(self, tenor):
        """The rebuilt contract's schedule (piecewisedefaultcurve.rs:403-417):
        it starts at the rolled protection start, as the helper's does, and ends
        a tenor past the evaluation date where the helper's ends a tenor past
        the protection start. The twentieth-IMM rule snaps both to the same
        twentieth.

        The maturity is left unadjusted, as the helper leaves its own
        (defaultprobabilityhelpers.rs:512). Rolling it under Following instead
        lengthens the 3Y contract by the two days from Saturday 20 June 2009,
        which shows up as a 3.6e-6 error on that pillar alone."""
        start = self.calendar.adjust(
            TODAY + SETTLEMENT_DAYS, BusinessDayConvention.Following
        )
        end = self.calendar.advance(
            TODAY, tenor, "Years", BusinessDayConvention.Unadjusted, False
        )
        return Schedule(
            start,
            end,
            Frequency.Quarterly,
            self.calendar,
            BusinessDayConvention.Following,
            DateGeneration.TwentiethIMM,
            termination_convention=BusinessDayConvention.Unadjusted,
        )

    def repriced_swap(self, quote, tenor):
        """A fresh contract on the market conventions, priced off the
        bootstrapped curve by a fresh engine."""
        swap = CreditDefaultSwap.with_terms(
            ProtectionSide.Buyer,
            1.0,
            quote,
            self.round_trip_schedule(tenor),
            BusinessDayConvention.Following,
            self.day_counter,
            self.settings,
            protection_start=TODAY + SETTLEMENT_DAYS,
        )
        swap.set_engine(
            MidPointCdsEngine(
                self.curve, RECOVERY_RATE, self.discount, self.settings
            )
        )
        return swap


def test_the_bootstrapped_curve_reproduces_every_input_cds_spread():
    market = Market()
    deviations = {
        tenor: abs(market.repriced_swap(quote, tenor).fair_spread() - quote)
        for quote, tenor in zip(QUOTES, TENORS)
    }
    print(f"max |fair_spread - quote| = {max(deviations.values()):.3e} {deviations}")
    assert max(deviations.values()) <= TOLERANCE


def test_the_bootstrap_lays_down_one_node_per_helper_plus_the_reference():
    market = Market()
    dates = market.curve.dates()
    assert len(dates) == len(market.helpers) + 1
    assert len(market.curve.nodes()) == len(dates)
    assert dates[0] == TODAY
    assert dates[1:] == [helper.latest_date() for helper in market.helpers]


def test_the_solved_curve_is_a_decreasing_survival_probability():
    market = Market()
    probabilities = [
        market.curve.survival_probability_date(date) for date in market.curve.dates()
    ]
    assert probabilities[0] == 1.0
    assert all(
        later < earlier for earlier, later in zip(probabilities, probabilities[1:])
    )
    assert all(rate > 0.0 for rate in market.curve.data())
    assert market.curve.times()[0] == 0.0


def test_an_empty_helper_list_is_refused():
    with pytest.raises(ItofinError, match="no bootstrap helpers"):
        PiecewiseDefaultCurve(TODAY, [], DayCounter.thirty360_bond_basis())
