"""CreditDefaultSwap priced by MidPointCdsEngine (#739).

The cached-value oracle mirrors the Rust one in
crates/libitofin/src/pricingengines/credit/midpointcdsengine.rs
(`a_ten_year_contract_matches_the_cached_mid_point_value`, itself
`testCachedValue` in creditdefaultswap.cpp:57-117): a ten-year contract on a
flat 1.234% hazard rate and a flat 6% continuous discount curve, seen from the
protection seller, is worth 295.0153398 and quotes a fair spread of
0.007517539081.

The two schedule dates are the C++ `calendar.advance(today, -1Y, Following)` and
`advance(issue, +10Y, Following)` hardcoded from a Rust probe of that fixture,
which printed `issue=2005-06-09 maturity=2015-06-09`. The Rust fixture builds its
schedule with `Schedule::new(.., DateGeneration::Forward, ..)` while `Schedule`
here goes through `MakeSchedule(..).forwards()`; a probe confirmed both produce
the same 21 dates and price identically, so the Python path is the same
contract, not a lookalike.
"""

import pytest
from itofin import ItofinError, Settings
from itofin.instruments import CreditDefaultSwap, ProtectionSide
from itofin.pricingengines import MidPointCdsEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward, FlatHazardRate
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    DayCounter,
    Frequency,
    Schedule,
)

TODAY = Date(9, 6, 2006)
ISSUE = Date(9, 6, 2005)
MATURITY = Date(9, 6, 2015)
HAZARD_RATE = 0.01234
DISCOUNT_RATE = 0.06
RECOVERY = 0.4
NOTIONAL = 10000.0
SPREAD = 0.0120
CACHED_NPV = 295.0153398
CACHED_FAIR_SPREAD = 0.007517539081
TOLERANCE = 1e-7


class Market:
    """The curves and conventions the C++ fixture shares across its cases."""

    def __init__(self):
        self.settings = Settings()
        self.settings.set_evaluation_date(TODAY)
        self.calendar = Calendar.target()
        self.day_counter = DayCounter.actual360()
        self.convention = BusinessDayConvention.ModifiedFollowing
        self.hazard = FlatHazardRate.moving(
            0,
            self.calendar,
            SimpleQuote(HAZARD_RATE),
            self.day_counter,
            self.settings,
        )
        self.discount = FlatForward(TODAY, DISCOUNT_RATE, self.day_counter)

    def engine(self) -> MidPointCdsEngine:
        return MidPointCdsEngine(self.hazard, RECOVERY, self.discount, self.settings)

    def schedule(self, start: Date, end: Date) -> Schedule:
        return Schedule(start, end, Frequency.Semiannual, self.calendar, self.convention)

    def contract(self, schedule: Schedule) -> CreditDefaultSwap:
        return CreditDefaultSwap(
            ProtectionSide.Seller,
            NOTIONAL,
            SPREAD,
            schedule,
            self.convention,
            self.day_counter,
            True,
            True,
            self.settings,
        )

    def contract_with_terms(self, schedule: Schedule, **terms) -> CreditDefaultSwap:
        return CreditDefaultSwap.with_terms(
            ProtectionSide.Seller,
            NOTIONAL,
            SPREAD,
            schedule,
            self.convention,
            self.day_counter,
            self.settings,
            **terms,
        )


def _priced(market: Market, cds: CreditDefaultSwap) -> CreditDefaultSwap:
    cds.set_engine(market.engine())
    return cds


def test_ten_year_contract_matches_the_cached_mid_point_value():
    market = Market()
    cds = _priced(market, market.contract(market.schedule(ISSUE, MATURITY)))

    assert abs(cds.npv() - CACHED_NPV) <= TOLERANCE
    assert abs(cds.fair_spread() - CACHED_FAIR_SPREAD) <= TOLERANCE


def test_the_two_leg_npvs_add_up_to_the_contract_npv():
    market = Market()
    cds = _priced(market, market.contract(market.schedule(ISSUE, MATURITY)))

    assert abs(cds.coupon_leg_npv() + cds.default_leg_npv() - cds.npv()) <= 1e-10


def test_with_terms_defaults_reproduce_the_cached_value():
    market = Market()
    cds = _priced(market, market.contract_with_terms(market.schedule(ISSUE, MATURITY)))

    assert abs(cds.npv() - CACHED_NPV) <= TOLERANCE
    assert abs(cds.fair_spread() - CACHED_FAIR_SPREAD) <= TOLERANCE


def test_an_earlier_protection_start_widens_the_first_default_window():
    """The engine takes the first live period's start from protection_start
    (midpointcdsengine.rs:191-195), then clamps it to today when today falls
    inside the period. A contract whose first period is already past therefore
    cannot see the argument at all, so this uses a forward-starting one: moving
    the start three months earlier widens the first default window and must move
    the price."""
    market = Market()
    schedule = market.schedule(Date(11, 12, 2006), Date(11, 12, 2011))
    default_start = _priced(market, market.contract_with_terms(schedule))
    early_start = _priced(
        market,
        market.contract_with_terms(schedule, protection_start=Date(11, 9, 2006)),
    )

    assert abs(early_start.npv() - default_start.npv()) > 1.0


def test_dropping_the_accrual_settlement_lowers_the_premium_leg():
    market = Market()
    schedule = market.schedule(ISSUE, MATURITY)
    settling = _priced(market, market.contract_with_terms(schedule))
    not_settling = _priced(
        market, market.contract_with_terms(schedule, settles_accrual=False)
    )

    assert not_settling.coupon_leg_npv() < settling.coupon_leg_npv()


def test_pricing_without_an_engine_raises():
    market = Market()
    cds = market.contract(market.schedule(ISSUE, MATURITY))

    with pytest.raises(ItofinError):
        cds.npv()
    with pytest.raises(ItofinError):
        cds.fair_spread()
