"""Mixed-strip bootstrap - the batch payoff (#550).

Mirrors the merged core oracle `mixed_strip_bootstraps`
(crates/libitofin/src/termstructures/yields/piecewiseyieldcurve.rs:521-632,
landed in #542) verbatim: a real market strip - deposits (1W/1M/3M) + a 3-month
IMM future + a 9x15 FRA + swaps (2Y/3Y/5Y) - bootstraps cleanly through the
Python facades and every instrument reprices its own quote off the solved curve.
The strip is copied exactly from the core test; it is arranged so pillar dates
are distinct and latest-relevant dates strictly monotone (the two ordering
invariants IterativeBootstrap enforces), and it is already PROVEN to bootstrap.
Do not alter the values: the bootstrapper errors by design on duplicate pillars
or non-monotone latest-relevant dates.

Euribor substitution (correct AND forced): the core builds the FRA/swap indexes
with `Euribor::six_months(Handle::empty(), settings)`; the pytest uses
`Euribor(Period(6, "Months"), None, settings)`. These are provably identical
(six_months -> named -> new, euribor.rs:98-119,48), and the substitution is
required because `PyEuribor.six_months(curve, settings)` takes a NON-optional
curve (hullwhite.rs:183-189), so an empty handle is unreachable through it.
"""

# itofin library
from itofin import Settings
from itofin.indexes import Euribor
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    DepositRateHelper,
    FraRateHelper,
    FuturesRateHelper,
    FuturesType,
    PiecewiseLogLinearDiscount,
    Pillar,
    SwapRateHelper,
)
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency
from itofin.time import Period as P
from itofin.time import next_imm_date

TOLERANCE = 1.0e-9

# (n, unit, rate) deposits, IMM future price, FRA rate and (n-years, rate) swaps,
# all transcribed from the core `mixed_strip_bootstraps` fixture.
DEPOSIT_DATA = [
    (1, "Weeks", 0.04559),
    (1, "Months", 0.04581),
    (3, "Months", 0.04557),
]
FUTURES_PRICE = 95.5
FRA_RATE = 0.046
SWAP_DATA = [
    (2, 0.0463),
    (3, 0.0475),
    (5, 0.0499),
]


def _strip():
    """The mixed strip and its curve (piecewiseyieldcurve.rs:528-606). TARGET
    calendar, evaluation date = adjust(15-Jun-2026, Following), settlement =
    today + 2 business days. Returns the settlement, the ordered helper list and
    the PiecewiseLogLinearDiscount curve (the T1 named class; the core uses
    <Discount, LogLinear>)."""
    calendar = Calendar.target()
    today = calendar.adjust(Date(15, 6, 2026), BusinessDayConvention.Following)
    settings = Settings()
    settings.set_evaluation_date(today)
    settlement = calendar.advance(
        today, 2, "Days", BusinessDayConvention.Following, False
    )

    helpers = []

    # Deposits (:543-552): SimpleQuote + Euribor(n, unit) over an empty handle.
    for n, unit, rate in DEPOSIT_DATA:
        index = Euribor(P(n, unit), None, settings)
        helpers.append(DepositRateHelper(SimpleQuote(rate), index))

    # Futures (:554-568): a 3-month IMM future starting at the next IMM date after
    # settlement (main_cycle False, matching imm::next_date(settlement, false)),
    # priced 95.5, length 3 months on TARGET/ModifiedFollowing, eom False,
    # Actual360, no convexity adjustment.
    imm_start = next_imm_date(settlement, False)
    helpers.append(
        FuturesRateHelper(
            SimpleQuote(FUTURES_PRICE),
            imm_start,
            3,
            calendar,
            BusinessDayConvention.ModifiedFollowing,
            False,
            DayCounter.actual360(),
            None,
            FuturesType.Imm,
        )
    )

    # FRA (:570-579): a 9x15 over a fresh 6M Euribor (empty handle), indexed
    # coupon, LastRelevantDate pillar.
    fra_index = Euribor(P(6, "Months"), None, settings)
    helpers.append(
        FraRateHelper(
            SimpleQuote(FRA_RATE),
            P(9, "Months"),
            fra_index,
            True,
            Pillar.LastRelevantDate,
        )
    )

    # Swaps (:581-597): annual/Unadjusted/Thirty360 bond-basis, floating off a
    # fresh 6M Euribor over an empty handle.
    for n, rate in SWAP_DATA:
        euribor6m = Euribor(P(6, "Months"), None, settings)
        helpers.append(
            SwapRateHelper(
                SimpleQuote(rate),
                P(n, "Years"),
                calendar,
                Frequency.Annual,
                BusinessDayConvention.Unadjusted,
                DayCounter.thirty360_bond_basis(),
                euribor6m,
            )
        )

    curve = PiecewiseLogLinearDiscount(settlement, helpers, DayCounter.actual360())
    return settlement, helpers, curve


def test_mixed_strip_bootstraps_and_reprices():
    """Port of `mixed_strip_bootstraps` (:610-631). Three arms:

    1. STRUCTURAL PIN (:611-616): one curve node per helper plus the reference
       node. `dates()` also forces the lazy bootstrap, so the reprice arm below
       reads a solved curve. This arm cannot be faked by the solver and catches a
       dropped or duplicated helper; it needs T1's `dates()` on the named class.
    2. WIRING/CONVERGENCE GATE, not a discriminating oracle (:618-628):
       quote_error = quote - implied_quote IS the bootstrap's own root
       (bootstraphelper.rs:317), solved to ~1e-12, so implied_quote re-asserts the
       solver's residual and would still pass a mis-wired quote. Arm 1 plus the
       per-helper oracles in the T2/T3 tests carry the discrimination.
    3. SANITY: discount factors strictly positive and strictly decreasing across
       the (increasing) helper maturity dates - a structural property the solver
       cannot fake.
    """
    _, helpers, curve = _strip()

    # Arm 1 - structural pin (forces the bootstrap).
    nodes = curve.dates()
    assert len(nodes) == len(helpers) + 1, (
        "one curve node per helper plus the reference"
    )

    # Arm 2 - wiring/convergence gate (see docstring: re-asserts the solver root).
    worst = 0.0
    for helper in helpers:
        implied = helper.implied_quote()
        quote = helper.quote_value()
        error = abs(implied - quote)
        worst = max(worst, error)
        assert error <= TOLERANCE, (
            f"mixed strip reprice: implied {implied} vs quote {quote} (err {error})"
        )
    assert worst <= TOLERANCE, f"worst mixed-strip reprice error {worst}"

    # Arm 3 - shape: strictly positive, strictly decreasing discount factors.
    previous = 1.0
    for helper in helpers:
        df = curve.discount_date(helper.maturity_date())
        assert 0.0 < df < previous, f"non-decreasing/negative df {df} after {previous}"
        previous = df
