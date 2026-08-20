"""CreditDefaultSwap.implied_hazard_rate on both pricing models (#817).

The mid-point arm mirrors the Rust test
`the_implied_hazard_rate_brackets_the_curve_and_reprices_the_contract` in
crates/libitofin/src/instruments/creditdefaultswap.rs: the same 15-Jun-2026
evaluation date, the same stepped 30%-to-40% hazard curve on unadjusted 5Y and
10Y nodes, the same flat 3% Act/360 discount curve, and the same 6Y-to-10Y
contracts struck off an issue date six months before today.

The Rust fixture builds its schedules with `Schedule::new(.., Forward, ..)`
while `Schedule` here goes through `MakeSchedule`. The two paths are shown to
build the same contracts rather than assumed to: the NPVs and implied rates
pinned below were printed from the Rust test itself, and Python reproduces every
one of them to the last bit. That is what makes the structural assertions
(bracket, monotone, reprice) an oracle instead of a restatement of whatever the
facade happens to return.

The ISDA arm is a new fixture, not a mirror: `implied_hazard_rate` routes
PricingModel.Isda into IsdaCdsEngine, which refuses curves that do not count
Act/365 (Fixed), so the mid-point fixture's Act/360 discount curve cannot be
reused. It borrows the proven Act/365F fixture from test_isda_cds.py instead.

Both arms need a weekday evaluation date, for different reasons. The solve
builds its own probability curve as a `FlatHazardRate` moving zero days off a
weekends-only calendar. On the mid-point arm a weekday is what lets that moving
curve and the fixed curve the round trip reprices on share a reference date, as
they do in C++, which adjusts todaysDate onto a business day; the Rust fixture's
own doc comment gives that as its reason for the date. On the ISDA arm the
engine additionally requires the probability curve's reference date to equal the
evaluation date, so on a weekend the moving reference lands past it, every
hazard guess fails to price, and Brent reports an opaque non-convergence.
15-Jun-2026 is a Monday and no TARGET holiday.
"""

# pypi/conda library
import pytest

# itofin library
from itofin import ItofinError, Settings
from itofin.instruments import CreditDefaultSwap, PricingModel, ProtectionSide
from itofin.pricingengines import IsdaCdsEngine, MidPointCdsEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward, FlatHazardRate, InterpolatedHazardRateCurve
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Schedule

TODAY = Date(15, 6, 2026)
H1 = 0.30
H2 = 0.40
DISCOUNT_RATE = 0.03
RECOVERY = 0.4
NOTIONAL = 10000.0
SPREAD = 0.0120
ACCURACY = 1.0e-8

# today + 5Y and today + 10Y as unadjusted date arithmetic, which is what the
# Rust fixture's `today + Period(n, Years)` does. Both land on a Sunday, so
# rolling them through the calendar would move the hazard step by a day.
STEP_DATE = Date(15, 6, 2031)
LAST_DATE = Date(15, 6, 2036)

# Printed from the Rust fixture, one row per 6Y-to-10Y contract: the maturity,
# the schedule size, the NPV off the stepped curve, and the implied rate the
# core solves for it.
RUST_ROWS = (
    (Date(15, 12, 2031), 13, -4304.466713157301, 0.30715503318212045),
    (Date(15, 12, 2032), 15, -4590.782802487836, 0.316990378125329),
    (Date(15, 12, 2033), 17, -4776.323330555159, 0.322646039773525),
    (Date(15, 12, 2034), 19, -4896.968852061268, 0.3255060511589049),
    (Date(17, 12, 2035), 21, -4975.76051807359, 0.3264744335603899),
)
ISSUE = Date(15, 12, 2025)

# The Rust mirror's own reprice bound. Brent bounds the root, not the objective,
# so the NPV residual is the rate error times dNPV/dh; on this fixture the
# largest measured residual is 4.8e-6, six orders inside the bound.
REPRICE_TOLERANCE = 1.0

# The solve is accurate to ACCURACY on the rate, and a flat source curve is
# byte-identical to the flat curve the solve builds, so an inversion must land
# within roughly ACCURACY of the rate it started from. Measured: 5.4e-10.
SOLVER_TOLERANCE = 1.0e-7


class SteppedMarket:
    """The Rust mid-point fixture, rebuilt through the facades."""

    def __init__(self):
        self.settings = Settings()
        self.settings.set_evaluation_date(TODAY)
        self.calendar = Calendar.target()
        self.day_counter = DayCounter.actual360()
        self.curve_day_counter = DayCounter.actual365_fixed()
        self.convention = BusinessDayConvention.ModifiedFollowing
        self.probability = InterpolatedHazardRateCurve(
            [TODAY, STEP_DATE, LAST_DATE], [H1, H1, H2], self.curve_day_counter
        )
        self.discount = FlatForward(TODAY, DISCOUNT_RATE, self.day_counter)

    def issue_date(self) -> Date:
        return self.calendar.advance(
            TODAY, -6, "Months", BusinessDayConvention.Following, False
        )

    def maturity(self, years: int) -> Date:
        return self.calendar.advance(
            self.issue_date(), years, "Years", BusinessDayConvention.Following, False
        )

    def schedule(self, years: int) -> Schedule:
        return Schedule(
            self.issue_date(),
            self.maturity(years),
            Frequency.Semiannual,
            self.calendar,
            self.convention,
        )

    def priced_on(self, years: int, probability) -> CreditDefaultSwap:
        cds = CreditDefaultSwap(
            ProtectionSide.Seller,
            NOTIONAL,
            SPREAD,
            self.schedule(years),
            self.convention,
            self.day_counter,
            True,
            True,
            self.settings,
        )
        cds.set_engine(
            MidPointCdsEngine(probability, RECOVERY, self.discount, self.settings)
        )
        return cds

    def invert(self, cds: CreditDefaultSwap, target_npv: float) -> float:
        return cds.implied_hazard_rate(
            target_npv,
            self.discount,
            self.curve_day_counter,
            RECOVERY,
            ACCURACY,
            PricingModel.Midpoint,
        )


def test_the_schedules_match_the_ones_the_rust_fixture_builds():
    """Pinned separately from the prices so a drift in the dates is told apart
    from a drift in the pricing."""
    market = SteppedMarket()

    assert market.issue_date() == ISSUE
    for years, (maturity, size, _, _) in zip(range(6, 11), RUST_ROWS):
        schedule = market.schedule(years)
        assert schedule.size() == size
        assert schedule.date(0) == ISSUE
        assert schedule.date(size - 1) == maturity


def test_the_stepped_curve_prices_to_the_rust_npvs():
    """The contracts being inverted are the Rust ones to the last bit, which is
    what lets the implied rates below be pinned as literals."""
    market = SteppedMarket()

    for years, (_, _, npv, _) in zip(range(6, 11), RUST_ROWS):
        assert market.priced_on(years, market.probability).npv() == npv


def test_the_implied_rates_reproduce_the_rust_ones():
    market = SteppedMarket()

    for years, (_, _, npv, rate) in zip(range(6, 11), RUST_ROWS):
        cds = market.priced_on(years, market.probability)
        assert market.invert(cds, npv) == rate


def test_the_implied_rates_bracket_the_curve_and_rise_with_maturity():
    """The mirrored structural oracle: a contract on a curve stepping from H1 to
    H2 has a flat-equivalent hazard rate between the two, and the further out the
    maturity the more of the higher step it sees."""
    market = SteppedMarket()
    previous = None

    for years, (_, _, npv, _) in zip(range(6, 11), RUST_ROWS):
        rate = market.invert(market.priced_on(years, market.probability), npv)
        assert H1 <= rate <= H2
        if previous is not None:
            assert rate >= previous
        previous = rate


def test_the_implied_rate_reprices_the_contract_on_a_flat_curve():
    market = SteppedMarket()

    for years, (_, _, npv, rate) in zip(range(6, 11), RUST_ROWS):
        flat = FlatHazardRate(TODAY, SimpleQuote(rate), market.curve_day_counter)
        reproduced = market.priced_on(years, flat).npv()
        assert abs(npv - reproduced) < REPRICE_TOLERANCE


def test_a_flat_curve_inverts_to_its_own_hazard_rate_on_the_mid_point_model():
    """An exact identity the bracket-and-reprice assertions cannot give: priced
    off a flat curve, the solve rebuilds that very curve, so the objective is
    zero at the rate it started from and the inversion recovers it."""
    market = SteppedMarket()

    for hazard_rate in (0.02, 0.05):
        flat = FlatHazardRate.with_rate(TODAY, hazard_rate, market.curve_day_counter)
        cds = market.priced_on(10, flat)
        implied = market.invert(cds, cds.npv())
        assert abs(implied - hazard_rate) < SOLVER_TOLERANCE


ISDA_MATURITY = Date(15, 6, 2029)
ISDA_HAZARD_RATE = 0.02

# The residual is the rate error times dNPV/dh, which this fixture carries as
# roughly -1.6e4 over the solved region; against a 5.4e-10 rate error that is
# some 9e-6, and the largest measured is 9.04e-6. The bound is an order above
# it and still four orders below anything a mis-routed model would move.
ISDA_REPRICE_TOLERANCE = 1.0e-4

# Two rates the same target NPV implies under the two models. Far enough apart
# to rule out PricingModel.Isda quietly pricing on the mid-point engine, which
# would collapse the gap to the solver's own 5.4e-10.
MODEL_GAP = 1.0e-7


class IsdaMarket:
    """The proven Act/365F fixture from test_isda_cds.py.

    Only the curves must count Act/365 (Fixed); the contract keeps its own
    Act/360 coupon day counter, as it does there.
    """

    def __init__(self, discount_day_counter: DayCounter | None = None):
        self.settings = Settings()
        self.settings.set_evaluation_date(TODAY)
        self.calendar = Calendar.target()
        self.day_counter = DayCounter.actual360()
        self.curve_day_counter = DayCounter.actual365_fixed()
        self.convention = BusinessDayConvention.Following
        self.discount = FlatForward(
            TODAY, DISCOUNT_RATE, discount_day_counter or self.curve_day_counter
        )

    def hazard(self, rate: float) -> FlatHazardRate:
        return FlatHazardRate.with_rate(TODAY, rate, self.curve_day_counter)

    def contract(self) -> CreditDefaultSwap:
        return CreditDefaultSwap(
            ProtectionSide.Seller,
            NOTIONAL,
            SPREAD,
            Schedule(
                TODAY,
                ISDA_MATURITY,
                Frequency.Quarterly,
                self.calendar,
                self.convention,
            ),
            self.convention,
            self.day_counter,
            True,
            True,
            self.settings,
        )

    def priced_on(self, rate: float) -> CreditDefaultSwap:
        cds = self.contract()
        cds.set_isda_engine(
            IsdaCdsEngine(self.hazard(rate), RECOVERY, self.discount, self.settings)
        )
        return cds

    def invert(
        self, cds: CreditDefaultSwap, target_npv: float, model: PricingModel
    ) -> float:
        return cds.implied_hazard_rate(
            target_npv,
            self.discount,
            self.curve_day_counter,
            RECOVERY,
            ACCURACY,
            model,
        )


def test_a_flat_curve_inverts_to_its_own_hazard_rate_on_the_isda_model():
    market = IsdaMarket()

    for hazard_rate in (ISDA_HAZARD_RATE, 0.05):
        cds = market.priced_on(hazard_rate)
        implied = market.invert(cds, cds.npv(), PricingModel.Isda)
        assert abs(implied - hazard_rate) < SOLVER_TOLERANCE


def test_the_isda_implied_rate_reprices_the_target_npv():
    market = IsdaMarket()
    cds = market.priced_on(ISDA_HAZARD_RATE)
    target_npv = cds.npv()

    implied = market.invert(cds, target_npv, PricingModel.Isda)
    reproduced = market.priced_on(implied).npv()

    assert abs(reproduced - target_npv) < ISDA_REPRICE_TOLERANCE


def test_the_two_models_invert_the_same_target_npv_differently():
    """The model argument routes to a different engine rather than being read and
    dropped: the ISDA arm recovers the curve it priced off, the mid-point arm
    lands elsewhere because it integrates the legs differently."""
    market = IsdaMarket()
    cds = market.priced_on(ISDA_HAZARD_RATE)
    target_npv = cds.npv()

    isda = market.invert(cds, target_npv, PricingModel.Isda)
    mid_point = market.invert(cds, target_npv, PricingModel.Midpoint)

    assert abs(isda - ISDA_HAZARD_RATE) < SOLVER_TOLERANCE
    assert abs(isda - mid_point) > MODEL_GAP


def test_the_isda_model_refuses_a_discount_curve_outside_its_conventions():
    """The ISDA engine's Act/365 (Fixed) requirement reaches the solver as a
    pricing failure at every guess, so it surfaces as a non-convergence rather
    than as the engine's own message."""
    market = IsdaMarket(discount_day_counter=DayCounter.actual360())
    cds = market.contract()

    with pytest.raises(ItofinError):
        market.invert(cds, 0.0, PricingModel.Isda)


def test_the_isda_model_refuses_a_day_counter_outside_its_conventions():
    """day_counter counts the probability curve the solve builds, and the engine
    requires Act/365 (Fixed) of that curve too, not only of the discount curve
    (isdacdsengine.rs:173-174). Documenting the requirement on one argument and
    not the other would send callers down the same opaque non-convergence."""
    market = IsdaMarket()
    cds = market.priced_on(ISDA_HAZARD_RATE)
    target_npv = cds.npv()

    with pytest.raises(ItofinError):
        cds.implied_hazard_rate(
            target_npv,
            market.discount,
            DayCounter.actual360(),
            RECOVERY,
            ACCURACY,
            PricingModel.Isda,
        )
