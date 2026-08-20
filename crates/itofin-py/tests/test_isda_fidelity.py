"""The three ISDA fidelity flags reach the price from Python (#814).

`test_isda_cds.py` shows that spelling the three kwargs out at their defaults
changes nothing. What is left to show is the other half: that each flag, when
flipped, moves the number - a flag read at the boundary but never threaded into
the core would price identically to one that was never read at all, and the
defaults-unchanged assertion alone could not tell the two apart.

Each flag needs a fixture it can be seen on, which is why there are three:

- AccrualBias is grid-independent, so the flat/flat fixture `test_isda_cds.py`
  already pins discriminates it.
- ForwardsInCouponPeriod subdivides each coupon period at the integration
  grid's own nodes, and two flat curves leave that grid as the maturity alone
  (isdanodegrid.rs:122-124), so it needs a curve with pillars strictly inside
  the coupon periods.
- NumericalFix only parts from the plain quotient where `f + h` reaches zero,
  which needs a discount rate cancelling the hazard.

Every number below was measured by running these very fixtures through the
facades on this machine; each is recorded against the Rust test whose mechanism
it mirrors, in crates/libitofin/src/pricingengines/credit/isdacdsengine.rs.
"""

# standard library
import math

# itofin library
from itofin import Settings
from itofin.instruments import CreditDefaultSwap, ProtectionSide
from itofin.pricingengines import AccrualBias, ForwardsInCouponPeriod, IsdaCdsEngine, NumericalFix
from itofin.termstructures import DiscountCurve, FlatForward, FlatHazardRate
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Schedule

TODAY = Date(15, 6, 2026)
NOTIONAL = 10000000.0
SPREAD = 0.01
RECOVERY = 0.4
HAZARD_RATE = 0.02
DISCOUNT_RATE = 0.03

# As in test_isda_cds.py: wide enough to survive a platform's exp differing in
# its last bit on values that run to 1e5, and still orders of magnitude below
# every delta asserted here.
TOLERANCE = 1e-8


def market_settings() -> Settings:
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    return settings


def contract(
    settings: Settings,
    maturity: Date,
    frequency: Frequency,
    day_counter: DayCounter,
) -> CreditDefaultSwap:
    """Sold protection on the ISDA-compatible terms, as in test_isda_cds.py.

    The seller's side is the one that leaves the premium leg its own sign, so
    the coupon-leg comparisons below read directly.
    """
    calendar = Calendar.target()
    convention = BusinessDayConvention.Following
    schedule = Schedule(TODAY, maturity, frequency, calendar, convention)
    return CreditDefaultSwap(
        ProtectionSide.Seller,
        NOTIONAL,
        SPREAD,
        schedule,
        convention,
        day_counter,
        True,
        True,
        settings,
    )


# --- AccrualBias, on test_isda_cds.py's flat/flat fixture --------------------

# Measured by running the fixture below through the facades. HalfDayBias
# reproduces the coupon leg test_isda_cds.py already pins, which is what says
# the default arm is untouched.
HALF_DAY_BIAS_COUPON_LEG = 281656.6267407311
NO_BIAS_COUPON_LEG = 281648.88829497696


def biased_coupon_leg(accrual_bias: AccrualBias) -> float:
    settings = market_settings()
    act365f = DayCounter.actual365_fixed()
    hazard = FlatHazardRate.with_rate(TODAY, HAZARD_RATE, act365f)
    discount = FlatForward(TODAY, DISCOUNT_RATE, act365f)
    cds = contract(
        settings, Date(15, 6, 2029), Frequency.Quarterly, DayCounter.actual360()
    )
    cds.set_isda_engine(
        IsdaCdsEngine(hazard, RECOVERY, discount, settings, accrual_bias=accrual_bias)
    )
    return cds.coupon_leg_npv()


def test_the_half_day_bias_moves_the_accrual():
    """The 1/730 of a year the biased setting shifts tstart back by is applied
    before the piecewise subdivision (isdacdsengine.rs:385-390), so it is
    grid-independent and shows on the flat/flat fixture where the other two
    flags cannot be seen.

    Worth about a day's accrual on each period, so the biased leg is strictly
    the larger: 281656.6267 against 281648.8883, a gap of 7.7384 - a relative
    2.75e-5, some three orders above this file's tolerance. Mirrors the Rust
    the_half_day_bias_moves_the_accrual (isdacdsengine.rs:1318), which asserts
    the same inequality on its own fixture.
    """
    biased = biased_coupon_leg(AccrualBias.HalfDayBias)
    unbiased = biased_coupon_leg(AccrualBias.NoBias)

    assert unbiased > 0.0
    assert biased > unbiased
    assert abs(biased - HALF_DAY_BIAS_COUPON_LEG) < TOLERANCE
    assert abs(unbiased - NO_BIAS_COUPON_LEG) < TOLERANCE
    assert abs(biased - unbiased) > 7.0


# --- ForwardsInCouponPeriod, on a stepped log-linear discount curve ----------

# Days from the evaluation date to each pillar, and the forward rate over each
# segment between them: the Rust premium-leg fixture's own PILLARS and FORWARDS
# (isdacdsengine.rs:1130-1136), which spread through the two years the contract
# runs so that pillars fall strictly inside the coupon periods. The last one
# reaches past the maturity, so nothing the engine reads is extrapolated.
PILLARS = [0, 30, 100, 200, 400, 800]
FORWARDS = [0.01, 0.05, 0.02, 0.07, 0.03]

# Measured by running the fixture below through the facades.
PIECEWISE_COUPON_LEG = 186395.69171210472
FLAT_COUPON_LEG = 186396.00296865826


def stepped_discount() -> DiscountCurve:
    """A log-linear discount curve whose forward rate changes from pillar to
    pillar, referenced at the evaluation date and counting Act/365 (Fixed).

    Log-linear in the discount factor is exactly the piecewise-constant-forward
    shape the ISDA grid accepts from a yield curve
    (isdanodegrid.rs:102-124), and it is the changes of forward that the
    piecewise flag needs: over a segment of constant forward the accrual
    integral is additive, so subdividing a flat curve returns the very number
    it started from.
    """
    dates = [TODAY]
    discounts = [1.0]
    log_discount = 0.0
    for segment, forward in enumerate(FORWARDS):
        log_discount -= forward * (PILLARS[segment + 1] - PILLARS[segment]) / 365.0
        dates.append(TODAY + PILLARS[segment + 1])
        discounts.append(math.exp(log_discount))
    return DiscountCurve(dates, discounts, DayCounter.actual365_fixed())


def stepped_coupon_leg(forwards: ForwardsInCouponPeriod) -> float:
    settings = market_settings()
    act365f = DayCounter.actual365_fixed()
    hazard = FlatHazardRate.with_rate(TODAY, HAZARD_RATE, act365f)
    cds = contract(settings, Date(15, 6, 2028), Frequency.Semiannual, act365f)
    cds.set_isda_engine(
        IsdaCdsEngine(
            hazard,
            RECOVERY,
            stepped_discount(),
            settings,
            accrual_bias=AccrualBias.NoBias,
            forwards_in_coupon_period=forwards,
        )
    )
    return cds.coupon_leg_npv()


def test_the_pillars_inside_a_coupon_period_move_the_accrual():
    """Subdividing a coupon period at the grid's own pillars reaches the
    accrual, which is the whole of what this flag selects.

    The fixture takes its shape from the Rust
    the_pillars_inside_a_coupon_period_move_the_accrual
    (isdacdsengine.rs:1344) - the same pillars and forwards, the same flat
    hazard, the same two-year semiannual contract counted Act/365 (Fixed) - but
    it is not that fixture rebuilt: the facades expose no weekends-only
    calendar, so the schedule rolls on TARGET where the Rust one rolls on
    WeekendsOnly. The dates therefore differ, and so do the values; neither
    number here can be reconciled against the Rust test's, and both were
    measured from this fixture.

    The grid is the discount pillars alone, a flat hazard curve contributing
    none (isdanodegrid.rs:90-93): 15 Jul 2026, 23 Sep 2026, 1 Jan 2027, 20 Jul
    2027 and 23 Aug 2028, against coupon periods opening 15 Jun 2026, 15 Dec
    2026, 15 Jun 2027 and 15 Dec 2027. Three of the five fall strictly inside a
    period - the last is past the maturity and is never reached - which is
    enough for the two settings to part.

    The bias is switched off so that what is measured is this flag alone.
    Measured: Piecewise 186395.6917 against Flat 186396.0030, a gap of 0.3113 -
    small next to the leg, being a second-order effect of curvature within a
    period, but a relative 1.67e-6 and so seven orders above the tolerance.
    """
    piecewise = stepped_coupon_leg(ForwardsInCouponPeriod.Piecewise)
    flat = stepped_coupon_leg(ForwardsInCouponPeriod.Flat)

    assert flat > 0.0
    assert abs(piecewise - PIECEWISE_COUPON_LEG) < TOLERANCE
    assert abs(flat - FLAT_COUPON_LEG) < TOLERANCE
    assert abs(piecewise - flat) / flat > 1e-7


# --- NumericalFix, on a discount rate cancelling the hazard ------------------

# The protection runs the 1096 days from the evaluation date to the maturity,
# counted Act/365 (Fixed) by both curves.
PROTECTION_YEARS = 1096 / 365.0

# The limit of h/(f+h) (1 - e^{-(f+h)t}) as f + h goes to zero is h t, so a leg
# integrating it comes to -h T N (1 - R) on the seller's side. Measured: the
# Taylor arm returns exactly this, to every digit.
CANCELLING_DEFAULT_LEG = -HAZARD_RATE * PROTECTION_YEARS * NOTIONAL * (1.0 - RECOVERY)


def cancelling_default_leg(numerical_fix: NumericalFix) -> float:
    """The protection leg on curves whose rates cancel.

    A discount curve at minus the hazard rate leaves `f + h` at zero, where the
    plain quotient divides a rounding error by the 10^-50 it adds and the
    Taylor series returns the limit. The negative rate passes the engine's
    checks, which ask only for Act/365 (Fixed) and a reference date at the
    evaluation date (isdacdsengine.rs:163-188) and say nothing about sign.
    """
    settings = market_settings()
    act365f = DayCounter.actual365_fixed()
    hazard = FlatHazardRate.with_rate(TODAY, HAZARD_RATE, act365f)
    discount = FlatForward(TODAY, -HAZARD_RATE, act365f)
    cds = contract(
        settings, Date(15, 6, 2029), Frequency.Quarterly, DayCounter.actual360()
    )
    cds.set_isda_engine(
        IsdaCdsEngine(hazard, RECOVERY, discount, settings, numerical_fix=numerical_fix)
    )
    return cds.default_leg_npv()


def test_the_series_returns_the_limit_the_quotient_cannot():
    """The one case in which the two arms of the numerical fix are told apart at
    all, mirroring the Rust the_series_returns_the_limit_the_quotient_cannot
    (isdacdsengine.rs:1069).

    Taylor is pinned twice over, deliberately: to the closed form -h T N (1 - R)
    it must reproduce, and to the literal -360328.7671232877 measured here,
    which the closed form matches to every digit. The literal is the regression
    pin and would survive an error introduced into the formula beside it; the
    formula is what says the number is the limit rather than whatever the arm
    happens to return. It is the protection leg that is read rather than the
    NPV: the premium leg runs through the same denominators, and the contract
    value under the unfixed arm comes to -1.26e52, a number no assertion should
    be built on.

    The unfixed arm is asserted only to be nowhere near the limit, as the Rust
    test asserts it, rather than pinned: what it returns is a rounding error
    divided by 10^-50, so whether it comes to the 0.0 measured here or to
    something enormous depends on whether a platform's exp(x) exp(-x) lands
    exactly on 1. Either outcome is far from the limit; neither is a number this
    port should be graded on.
    """
    taylor = cancelling_default_leg(NumericalFix.Taylor)
    quotient = cancelling_default_leg(NumericalFix.NoFix)

    assert abs(taylor - CANCELLING_DEFAULT_LEG) < TOLERANCE
    assert abs(taylor + 360328.7671232877) < TOLERANCE
    assert abs(quotient - taylor) > 0.5 * abs(taylor)
