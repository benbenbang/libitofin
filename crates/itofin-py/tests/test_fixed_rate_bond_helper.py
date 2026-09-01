"""FixedRateBondHelper and BondPriceType - facing the bond leg of the bootstrap (#530).

A bootstrap round trip alone is VACUOUS for this facade: the solver moves the
curve until the helper's own bond reprices to its quote, so a mis-routed
argument (a swapped face/redemption, a wrong coupon scale, a dropped
price_type) just shifts the curve and the round trip stays green. The external
truth therefore lives in the first test, which hand-derives the quote from a
flat curve and demands the bootstrap give that flat curve back.

Four tests:

1. `test_single_period_bond_recovers_the_flat_curve` - EXTERNAL TRUTH. Every
   cash flow sits on the pillar, so the recovered discount factor is an exact
   algebraic function of the quote, with no interpolation in between.
2. `test_bond_bootstrap_reprices_the_quotes` - the round trip, a port of the
   core oracle `bond_bootstrap_reprices_the_quotes`
   (crates/libitofin/src/termstructures/yields/bondhelpers.rs:467) over its own
   five-bond fixture (:473-479).
3. `test_clean_and_dirty_quotes_differ_by_the_accrued_amount` - TEETH for
   price_type, with the accrued > 0 guard the arm needs to be non-degenerate.
4. `test_a_future_issue_date_floors_the_settlement_date` - keeps issue_date from
   being an accepted-and-ignored argument.
"""

# itofin library
from itofin import Settings
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    BondPriceType,
    FixedRateBondHelper,
    FlatForward,
    PiecewiseLogLinearDiscount,
)
from itofin.time import BusinessDayConvention as Bdc
from itofin.time import Calendar, Date
from itofin.time import DateGeneration as Rule
from itofin.time import DayCounter, Frequency, Schedule


def test_single_period_bond_recovers_the_flat_curve():
    """EXTERNAL TRUTH: a flat 4% curve, hand-priced into a quote, is recovered.

    The bond has one period, its settlement is the evaluation date (zero
    settlement days) and the curve reference is that same date, so the whole
    valuation collapses to

        quote = (coupon + redemption amount) * D(payment) * 100 / face

    with no discounting to settlement and no interpolation between nodes: the
    bootstrap's single unknown D(pillar) is pinned algebraically by the quote.
    The recovered node therefore has to be the flat curve's own discount factor
    to machine precision, and the measured error is 2.2e-16.

    Fixture literals and the mutant each kills (measured by re-running the
    bootstrap with the argument perturbed, error in the recovered node):

    - `REDEMPTION = 102.0`, distinct from the face and from the coupon scale.
      Killed mutants: redemption defaulted to 100.0 (1.8e-2), and face and
      redemption swapped in the ctor call (5.6e-1). `FACE = 250.0` is NOT a
      pinned literal on its own: a single-notional bond's price per 100 of face
      is invariant under the face amount, which cancels out of the coupon, the
      redemption and the per-100 scaling alike (measured: 0.0). What the pair of
      distinct values pins is the ORDER of the two arguments.
    - `RATE = 0.05` through the `coupons` list. Killed mutant: a wrong coupon
      scale (0.04 instead of 0.05: 9.2e-3).
    - `DAY_COUNTER = Actual/360`, which makes the accrual factor 365/360 rather
      than 1. Killed mutant: the day counter dropped for Actual/365 Fixed
      (6.2e-4).
    - `Bdc.Following` as the payment convention against an UNADJUSTED schedule
      whose 19-Jun-2027 end is a Saturday. The pillar is the payment date, so it
      lands on Monday 21-Jun-2027 and the explicit `pillar_date()` assertion
      below plus the node comparison both bite. Killed mutant: the payment
      convention dropped for Unadjusted (pillar 19-Jun-2027, 2.1e-4).
    - `settlement_days = 0`. Killed mutant: a hard-coded settlement lag (3 days:
      1.2e-4).

    Two arguments are deliberately NOT pinned here: `price_type`, because the
    settlement date equals the accrual start so the accrued amount is zero and
    clean equals dirty (test 3 carries it), and `issue_date`, because it only
    floors the settlement date and here it is not later than it (test 4 carries
    it).
    """
    today = Date(19, 6, 2026)
    settings = Settings()
    settings.set_evaluation_date(today)

    calendar = Calendar.target()
    day_counter = DayCounter.actual360()
    end = Date(19, 6, 2027)
    schedule = Schedule(
        today,
        end,
        Frequency.Annual,
        calendar,
        Bdc.Unadjusted,
        Rule.Backward,
        Bdc.Unadjusted,
    )
    assert [str(d) for d in schedule.dates()] == ["Date(19, 6, 2026)", "Date(19, 6, 2027)"], (
        "the fixture is single-period: one coupon, paid with the redemption"
    )

    face = 250.0
    redemption = 102.0
    rate = 0.05
    payment = Date(21, 6, 2027)

    flat = FlatForward(today, 0.04, day_counter)
    coupon_amount = face * rate * day_counter.year_fraction(today, end)
    redemption_amount = face * redemption / 100.0
    quote = (coupon_amount + redemption_amount) * flat.discount_date(payment) * 100.0 / face
    assert abs(coupon_amount - 12.673611111111111) < 1e-12, "250 * 5% over 365/360"
    assert abs(quote - 102.79121165871838) < 1e-12, "the hand-derived clean quote"

    helper = FixedRateBondHelper(
        SimpleQuote(quote),
        0,
        face,
        schedule,
        [rate],
        day_counter,
        Bdc.Following,
        redemption,
        BondPriceType.Clean,
        settings,
        today,
    )
    assert helper.pillar_date() == payment, (
        "the pillar is the last cash-flow date, rolled off the Saturday maturity by "
        "the payment convention"
    )

    curve = PiecewiseLogLinearDiscount(today, [helper], day_counter)
    assert len(curve.dates()) == 2, "the reference node plus this helper's node"

    recovered = curve.discount_date(payment)
    expected = flat.discount_date(payment)
    assert abs(recovered - expected) < 1e-12, (
        f"recovered {recovered} vs flat {expected}"
    )
    assert abs(curve.zero_rate(day_counter.year_fraction(today, payment)) - 0.04) < 1e-12, (
        "the recovered continuously compounded zero is the curve's own 4%"
    )


# `piecewiseyieldcurve.cpp testCurveConsistency` bonds section, transcribed from
# the core const BOND_DATA (bondhelpers.rs:473-479): (tenor length, unit,
# seasoning in years, coupon as a PERCENT, quoted clean price).
BOND_DATA = [
    (6, "Months", 5, 4.75, 101.320),
    (1, "Years", 3, 2.75, 100.590),
    (2, "Years", 5, 5.00, 105.650),
    (5, "Years", 11, 5.50, 113.610),
    (10, "Years", 11, 3.75, 104.070),
]
BOND_SETTLEMENT_DAYS = 3
ROUND_TRIP_TOLERANCE = 1.0e-9


def test_bond_bootstrap_reprices_the_quotes():
    """ROUND TRIP: a curve bootstrapped purely from bond helpers reprices them.

    The port of `bond_bootstrap_reprices_the_quotes` (bondhelpers.rs:467) with
    its fixture kept literal: the three date bases stay distinct as in the C++
    `CommonVars` (today = TARGET.adjust(evaluation date), the curve reference
    settlement = today + 2 business days, and three bond settlement days), each
    bond is seasoned so its schedule starts in the past, and the curve is the
    `<Discount, LogLinear>` of piecewiseyieldcurve.cpp:683. The repricing runs
    through `implied_quote`, which is the bootstrap's own root
    (bootstraphelper.rs:317), so this arm is SELF-CONSISTENCY: it proves the
    multi-coupon, seasoned, Actual/Actual ISDA path assembles and converges, and
    it is deliberately blind to the argument wiring that the first test pins.
    The worst measured error is 4.3e-14, well inside the core's 1e-9.

    Two arms the solver cannot fake accompany it: the node count (one per helper
    plus the reference), which also forces the lazy bootstrap so the repricing
    below reads a solved curve, and the shape (discount factors strictly
    positive and strictly decreasing across the increasing maturities).
    """
    calendar = Calendar.target()
    today = calendar.adjust(Date(15, 6, 2026), Bdc.Following)
    settings = Settings()
    settings.set_evaluation_date(today)
    settlement = calendar.advance(today, 2, "Days", Bdc.Following, False)
    bond_day_counter = DayCounter.actual_actual_isda()

    helpers = []
    for n, unit, length, coupon, price in BOND_DATA:
        maturity = calendar.advance(today, n, unit, Bdc.Following, False)
        issue = calendar.advance(maturity, -length, "Years", Bdc.Following, False)
        schedule = Schedule(issue, maturity, Frequency.Semiannual, calendar, Bdc.Following, Rule.Backward)
        helpers.append(
            FixedRateBondHelper(
                SimpleQuote(price),
                BOND_SETTLEMENT_DAYS,
                100.0,
                schedule,
                [coupon / 100.0],
                bond_day_counter,
                Bdc.Following,
                100.0,
                BondPriceType.Clean,
                settings,
                issue,
            )
        )

    curve = PiecewiseLogLinearDiscount(settlement, helpers, DayCounter.actual360())

    nodes = curve.dates()
    assert len(nodes) == len(helpers) + 1, "one curve node per helper plus the reference"

    worst = 0.0
    for helper, (n, unit, _length, _coupon, price) in zip(helpers, BOND_DATA, strict=True):
        estimated = helper.implied_quote()
        error = abs(estimated - price)
        worst = max(worst, error)
        assert error < ROUND_TRIP_TOLERANCE, (
            f"{n} {unit} bond: estimated {estimated} vs expected {price} (error {error})"
        )
    assert worst < ROUND_TRIP_TOLERANCE, f"worst bond reprice error {worst}"

    previous = 1.0
    for helper in helpers:
        df = curve.discount_date(helper.maturity_date())
        assert 0.0 < df < previous, f"non-decreasing/negative df {df} after {previous}"
        previous = df


def _seasoned_helper(quote, price_type, settings, schedule, calendar, day_counter):
    """The five-year semiannual 5% bond of the core fixture (bondhelpers.rs:246-285),
    issued a year before the evaluation date and settling three business days out."""
    return FixedRateBondHelper(
        SimpleQuote(quote),
        3,
        100.0,
        schedule,
        [0.05],
        day_counter,
        Bdc.Following,
        100.0,
        price_type,
        settings,
        Date(15, 6, 2025),
    )


def test_clean_and_dirty_quotes_differ_by_the_accrued_amount():
    """TEETH for price_type, with the non-degeneracy guard the arm needs.

    The same bond, the same quoted number, read once as a clean price and once
    as a dirty one, must bootstrap DIFFERENT curves - and must differ by exactly
    the accrued amount at settlement. Both halves matter:

    - Without the difference assertion the arm cannot see a price_type wired to
      a constant; with it, a `Clean`-always mutant collapses the two curves and
      the assertion fails. Measured separation at the pillar: 3.7e-4 in the
      discount factor.
    - Without the accrued > 0 guard the arm would be silently vacuous on a bond
      settling on a coupon date, where clean equals dirty and the two curves
      coincide whatever price_type does (the equal-under-fixture family). The
      fixture puts settlement three business days past the 15-Jun-2026 coupon
      date, so it accrues, and the guard below asserts it.

    The accrued amount is hand-derived rather than read back from the facade,
    which exposes no bond: a fixed-rate coupon accrues simply, not compounded
    (fixedratecoupon.rs:88 builds its rate with `Compounding::Simple`), so per
    100 of notional it is `100 * rate * yearFraction(accrual start, settlement)`
    = 100 * 0.05 * 3/360 = 0.041666...  Feeding `quote + accrued` to a `Dirty`
    helper must then land on exactly the curve the `Clean` helper at `quote`
    produced, which pins the accrued amount to the last bit rather than merely
    asserting the two differ. Measured agreement: 0.0.

    The accrual start and the settlement date are both asserted against the
    already-tested schedule and calendar facades, so the derivation is machine
    checked rather than asserted from a mental calendar.
    """
    today = Date(15, 6, 2026)
    settings = Settings()
    settings.set_evaluation_date(today)

    calendar = Calendar.target()
    day_counter = DayCounter.actual360()
    schedule = Schedule(
        Date(15, 6, 2025),
        Date(15, 6, 2030),
        Frequency.Semiannual,
        calendar,
        Bdc.Following,
        Rule.Backward,
    )

    accrual_start = schedule.date(2)
    assert accrual_start == Date(15, 6, 2026), "the coupon period containing settlement starts here"
    settlement = calendar.advance(today, 3, "Days", Bdc.Following, False)
    assert settlement == Date(18, 6, 2026), "three TARGET business days past the evaluation date"

    accrued = 100.0 * 0.05 * day_counter.year_fraction(accrual_start, settlement)
    assert accrued > 0.0, "the bond is mid-coupon and accruing, so clean and dirty differ"
    assert abs(accrued - 0.041666666666666664) < 1e-15, "100 * 5% over 3/360"

    quote = 100.0

    def pillar_df(helper):
        curve = PiecewiseLogLinearDiscount(today, [helper], day_counter)
        return curve.discount_date(helper.pillar_date())

    clean = _seasoned_helper(quote, BondPriceType.Clean, settings, schedule, calendar, day_counter)
    dirty = _seasoned_helper(quote, BondPriceType.Dirty, settings, schedule, calendar, day_counter)
    shifted = _seasoned_helper(
        quote + accrued, BondPriceType.Dirty, settings, schedule, calendar, day_counter
    )

    assert clean.pillar_date() == Date(17, 6, 2030), (
        "the redemption rolls off the 2030-06-15 Saturday maturity to the Monday"
    )

    clean_df = pillar_df(clean)
    dirty_df = pillar_df(dirty)
    shifted_df = pillar_df(shifted)

    assert abs(clean_df - dirty_df) > 1.0e-6, (
        f"price_type must move the bootstrap: clean {clean_df} vs dirty {dirty_df}"
    )
    assert dirty_df < clean_df, (
        "the same number read as a dirty price buys less bond, so the curve discounts harder"
    )
    assert abs(shifted_df - clean_df) < 1.0e-12, (
        f"a dirty quote of clean + accrued must reproduce the clean curve: "
        f"{shifted_df} vs {clean_df}"
    )


def test_a_future_issue_date_floors_the_settlement_date():
    """issue_date is wired, not accepted and ignored.

    A bond cannot settle before it is issued, so an issue date past the ordinary
    settlement date pushes settlement out and moves the bootstrapped curve. The
    single-period fixture of the first test is reused with the issue date shifted
    a month past the schedule start, which separates the pillar discount factor
    by 1.2e-4; a facade that dropped issue_date, or hard-coded it to None, would
    give the two runs the same curve and fail this test. The `None` run is
    asserted equal to the issue-at-start run because an issue date not later than
    the settlement date has no effect, which is what makes the shifted run the
    discriminating one.
    """
    today = Date(19, 6, 2026)
    settings = Settings()
    settings.set_evaluation_date(today)

    calendar = Calendar.target()
    day_counter = DayCounter.actual360()
    schedule = Schedule(
        today,
        Date(19, 6, 2027),
        Frequency.Annual,
        calendar,
        Bdc.Unadjusted,
        Rule.Backward,
        Bdc.Unadjusted,
    )

    def pillar_df(issue_date):
        helper = FixedRateBondHelper(
            SimpleQuote(102.0),
            0,
            250.0,
            schedule,
            [0.05],
            day_counter,
            Bdc.Following,
            102.0,
            BondPriceType.Clean,
            settings,
            issue_date,
        )
        curve = PiecewiseLogLinearDiscount(today, [helper], day_counter)
        return curve.discount_date(helper.pillar_date())

    at_start = pillar_df(today)
    unset = pillar_df(None)
    later = pillar_df(Date(20, 7, 2026))

    assert abs(at_start - unset) < 1e-15, "an issue date at the schedule start is the unset case"
    assert abs(later - at_start) > 1.0e-6, (
        f"a future issue date must floor the settlement and move the curve: {later} vs {at_start}"
    )
