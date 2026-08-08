"""CreditDefaultSwap priced by IsdaCdsEngine (#812).

The three pinned values come from the Rust test
`the_flat_curve_fixture_prices_to_the_pinned_value` in the `binding_oracle`
module of crates/libitofin/src/pricingengines/credit/isdacdsengine.rs, which
records them from its own run. That fixture is rebuilt here byte-for-byte - the
same evaluation date, the same two flat Act/365F curves referenced at it, the
same TARGET/Following/Forward quarterly schedule and the same contract terms -
so the literals grade the facade rather than restate whatever it happens to
return.

The schedule shape is asserted on both sides (13 dates, first the evaluation
date, last the maturity) so a drift in the dates is told apart from a drift in
the pricing, and the two legs are pinned alongside the value so a mismatch says
which half moved.

Two flat curves leave the ISDA integration grid as the maturity alone
(isdanodegrid.rs:122-124), which is still a genuine single-node ISDA reprice and
still distinct from the mid-point engine's period-by-period integration - hence
the discriminator below.
"""

import pytest
from itofin import ItofinError, Settings
from itofin.instruments import CreditDefaultSwap, ProtectionSide
from itofin.pricingengines import IsdaCdsEngine, MidPointCdsEngine
from itofin.termstructures import FlatForward, FlatHazardRate
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    DayCounter,
    Frequency,
    Schedule,
)

TODAY = Date(15, 6, 2026)
MATURITY = Date(15, 6, 2029)
HAZARD_RATE = 0.02
DISCOUNT_RATE = 0.03
RECOVERY = 0.4
NOTIONAL = 10000000.0
SPREAD = 0.01

NPV = -52927.18294373818
COUPON_LEG_NPV = 281656.6267407311
DEFAULT_LEG_NPV = -334583.8096844693

# Wide enough to survive a platform's exp differing in its last bit: the values
# run to 1e5, so this is a relative 2e-13, while the literals were recorded on
# one machine and are asserted on whichever one CI happens to run. Still some
# nine orders of magnitude below anything a mis-wired engine, curve or contract
# term would move the price by.
TOLERANCE = 1e-8


class Market:
    """The Rust fixture's curves and conventions, rebuilt through the facades.

    day_counter is the contract's own Act/360; the two curves count Act/365
    (Fixed), which is what the ISDA engine requires of them.
    """

    def __init__(self, curve_day_counter: DayCounter | None = None):
        self.settings = Settings()
        self.settings.set_evaluation_date(TODAY)
        self.calendar = Calendar.target()
        self.day_counter = DayCounter.actual360()
        self.convention = BusinessDayConvention.Following
        curves = curve_day_counter or DayCounter.actual365_fixed()
        self.hazard = FlatHazardRate.with_rate(TODAY, HAZARD_RATE, curves)
        self.discount = FlatForward(TODAY, DISCOUNT_RATE, curves)

    def isda_engine(self) -> IsdaCdsEngine:
        return IsdaCdsEngine(self.hazard, RECOVERY, self.discount, self.settings)

    def mid_point_engine(self) -> MidPointCdsEngine:
        return MidPointCdsEngine(self.hazard, RECOVERY, self.discount, self.settings)

    def schedule(self) -> Schedule:
        return Schedule(
            TODAY, MATURITY, Frequency.Quarterly, self.calendar, self.convention
        )

    def contract(self) -> CreditDefaultSwap:
        return CreditDefaultSwap(
            ProtectionSide.Seller,
            NOTIONAL,
            SPREAD,
            self.schedule(),
            self.convention,
            self.day_counter,
            True,
            True,
            self.settings,
        )


def test_the_schedule_matches_the_one_the_rust_fixture_builds():
    schedule = Market().schedule()

    assert schedule.size() == 13
    assert schedule.date(0) == TODAY
    assert schedule.date(12) == MATURITY


def test_the_flat_curve_contract_prices_to_the_rust_pinned_value():
    market = Market()
    cds = market.contract()
    cds.set_isda_engine(market.isda_engine())

    assert abs(cds.npv() - NPV) < TOLERANCE
    assert abs(cds.coupon_leg_npv() - COUPON_LEG_NPV) < TOLERANCE
    assert abs(cds.default_leg_npv() - DEFAULT_LEG_NPV) < TOLERANCE


def test_the_isda_price_differs_meaningfully_from_the_mid_point_price():
    """The same contract on the same curves, priced by the other engine. The gap
    is asserted as a relative size rather than as an inequality so that two
    values agreeing to the last bit could not pass as a difference."""
    market = Market()
    isda = market.contract()
    isda.set_isda_engine(market.isda_engine())
    mid_point = market.contract()
    mid_point.set_engine(market.mid_point_engine())

    npv_isda = isda.npv()
    npv_mid = mid_point.npv()

    assert abs(npv_isda - npv_mid) / abs(npv_mid) > 1e-6


def test_curves_outside_the_isda_conventions_are_refused_when_pricing():
    """Construction is infallible, so the Act/365 (Fixed) requirement surfaces
    from npv(), where the engine validates its inputs."""
    market = Market(curve_day_counter=DayCounter.actual360())
    cds = market.contract()
    cds.set_isda_engine(market.isda_engine())

    with pytest.raises(ItofinError):
        cds.npv()


def test_the_mid_point_engine_accepts_the_curves_the_isda_engine_refuses():
    """The refusal above is the ISDA engine's own check, not something about the
    Act/360 fixture: the mid-point engine prices that same fixture to
    -57557.43897416495.

    The sign and the magnitude are pinned rather than the digits, which belong
    to the mid-point engine and are not this test's subject: the contract is
    sold protection at a spread below the fair one, so the value is negative and
    is tens of thousands on a ten-million notional. A price of zero, of the
    wrong sign, or rounding to nothing would all mean the engine had not really
    priced the contract, which is the only thing being shown here.
    """
    market = Market(curve_day_counter=DayCounter.actual360())
    cds = market.contract()
    cds.set_engine(market.mid_point_engine())

    npv = cds.npv()

    assert npv < 0.0
    assert 1e4 < abs(npv) < 1e5
