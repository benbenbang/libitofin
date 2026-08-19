"""OIS rate-helper bootstrap - facing the overnight strip (#551, #554).

Mirrors the merged core oracle `ois_bootstrap_reprices_the_quotes`
(crates/libitofin/src/termstructures/yields/ratehelpers.rs:1677-1753): an ESTR
discounting curve bootstrapped purely from OISRateHelpers reprices every OIS
quote it was built from.

The reprice arm is the core loop (ratehelpers.rs:1729-1752) ported verbatim:
each quote is repriced by an independently built OvernightIndexedSwap
(MakeOis + fair_rate()) floating off a FRESH Estr forwarding on the
BOOTSTRAPPED curve, not off the empty-handle index the helpers were built with.
That is a real instrument valuation, unlike the helper-level
abs(implied_quote - quote_value) it replaces (#554), which was the bootstrap's
OWN root (bootstraphelper.rs:317) and so re-asserted the solver residual rather
than discriminating a mis-wire.
"""

import pytest

from itofin import Settings
from itofin.indexes import Estr
from itofin.instruments import MakeOis
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    FlatForward,
    OISRateHelper,
    PiecewiseLogLinearDiscount,
    Pillar,
    RateAveraging,
)
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency
from itofin.time import Period as P

TOLERANCE = 1.0e-8

# `overnightindexedswap.cpp estrSwapData` (:92-125), transcribed verbatim from
# the core const ESTR_SWAP_DATA (ratehelpers.rs:1626-1659): (tenor length, unit,
# rate as a PERCENT). All 33 use two settlement days. The rates are percentages;
# the helper is fed rate / 100.0, exactly as the core does at ratehelpers.rs:1701.
ESTR_SWAP_DATA = [
    (1, "Weeks", 1.245),
    (2, "Weeks", 1.269),
    (3, "Weeks", 1.277),
    (1, "Months", 1.281),
    (2, "Months", 1.18),
    (3, "Months", 1.143),
    (4, "Months", 1.125),
    (5, "Months", 1.116),
    (6, "Months", 1.111),
    (7, "Months", 1.109),
    (8, "Months", 1.111),
    (9, "Months", 1.117),
    (10, "Months", 1.129),
    (11, "Months", 1.141),
    (12, "Months", 1.153),
    (15, "Months", 1.218),
    (18, "Months", 1.308),
    (21, "Months", 1.407),
    (2, "Years", 1.510),
    (3, "Years", 1.916),
    (4, "Years", 2.254),
    (5, "Years", 2.523),
    (6, "Years", 2.746),
    (7, "Years", 2.934),
    (8, "Years", 3.092),
    (9, "Years", 3.231),
    (10, "Years", 3.380),
    (11, "Years", 3.457),
    (12, "Years", 3.544),
    (15, "Years", 3.702),
    (20, "Years", 3.703),
    (25, "Years", 3.541),
    (30, "Years", 3.369),
]

PAYMENT_LAG = 2


def _curve():
    """The ESTR bootstrap of `ois_bootstrap_reprices_the_quotes` (:1685-1725):
    today = 5-Feb-2009, TARGET, an Estr forwarding off an empty handle, one
    OISRateHelper per quote, and a PiecewiseLogLinearDiscount whose reference is
    TODAY (not settlement, :1719-1720) on Actual/365 Fixed. Returns the ordered
    helper list, the curve, the settings the reprice arm must share (D5: a fresh
    Settings has no evaluation date, so the engine MakeOis attaches would fail on
    it) and the settlement date the repriced swaps start on (:1689-1695)."""
    today = Date(5, 2, 2009)
    settings = Settings()
    settings.set_evaluation_date(today)
    calendar = Calendar.target()
    settlement = calendar.advance(
        today, 2, "Days", BusinessDayConvention.Following, False
    )

    estr = Estr(None, settings)
    helpers = []
    for n, unit, rate in ESTR_SWAP_DATA:
        quote = SimpleQuote(rate / 100.0)
        helpers.append(
            OISRateHelper(
                2,
                P(n, unit),
                quote,
                estr,
                PAYMENT_LAG,
                BusinessDayConvention.Following,
                Frequency.Annual,
                P(0, "Days"),
                settings,
                None,
                None,
                Pillar.LastRelevantDate,
                RateAveraging.Compound,
            )
        )

    curve = PiecewiseLogLinearDiscount(today, helpers, DayCounter.actual365_fixed())
    return helpers, curve, settings, settlement


def test_ois_bootstrap_reprices_the_quotes():
    """Port of `ois_bootstrap_reprices_the_quotes`. Three arms:

    1. STRUCTURAL PIN: one curve node per helper plus the reference node.
       `dates()` also forces the lazy bootstrap, so the reprice arm below reads a
       solved curve. This arm cannot be faked by the solver.
    2. REPRICE (ratehelpers.rs:1729-1752): every quote is recovered as the fair
       rate of an independently built OIS floating off a fresh Estr on the
       bootstrapped curve. It discriminates the wiring the bootstrap root cannot:
       the payment lag, the averaging method and the fresh-index-on-the-solved-
       curve forwarding each move the fair rate off the quote when mis-set. The
       nominal is asserted separately - fair_rate() is nominal-invariant, so the
       reprice alone would not catch a dropped with_nominal (the core default is
       1.0). Two of the knobs passed here are degenerate under this fixture and
       are NOT pinned, though the core loop passes them too: effective_date,
       because MakeOis derives the same 9-Feb-2009 start from its own two-day
       family default (makeois.rs:526-534 - the ESTR family, not the index's own
       zero settlement days), and discounting_term_structure, because the curve
       passed is already the index's forwarding curve.
    3. SHAPE: discount factors strictly positive and strictly decreasing across
       the (increasing) helper maturity dates - a structural property the solver
       cannot fake.
    """
    helpers, curve, settings, settlement = _curve()

    # Arm 1 - structural pin (forces the bootstrap).
    nodes = curve.dates()
    assert len(nodes) == len(helpers) + 1, (
        "one curve node per helper plus the reference"
    )

    # Arm 2 - reprice each quote with an independently built OIS.
    worst = 0.0
    for n, unit, rate in ESTR_SWAP_DATA:
        priced_estr = Estr(curve, settings)
        swap = MakeOis(
            P(n, unit),
            priced_estr,
            settings,
            fixed_rate=0.0,
            forward_start=P(0, "Days"),
            effective_date=settlement,
            nominal=100.0,
            payment_lag=PAYMENT_LAG,
            discounting_term_structure=curve,
            averaging_method=RateAveraging.Compound,
        ).build()
        assert swap.nominal() == 100.0, "the builder nominal reached the swap"
        calculated = swap.fair_rate()
        expected = rate / 100.0
        error = abs(calculated - expected)
        worst = max(worst, error)
        assert error < TOLERANCE, (
            f"{n} {unit} OIS: calculated {calculated} vs expected {expected}"
        )
    assert worst < TOLERANCE, f"worst OIS reprice error {worst}"

    # Arm 3 - shape: strictly positive, strictly decreasing discount factors.
    previous = 1.0
    for helper in helpers:
        df = curve.discount_date(helper.maturity_date())
        assert 0.0 < df < previous, f"non-decreasing/negative df {df} after {previous}"
        previous = df


def test_make_ois_without_a_fixed_rate_prices_at_par():
    """`fixed_rate=None` is the C++ `Null<Rate>()` default: MakeOis assembles a
    temporary swap at zero and writes its fair rate into the fixed leg
    (makeois.rs:461-469), so the built swap prices to a zero NPV by construction.
    Complements the reprice arm, which fixes the rate at 0.0 and reads the fair
    rate back."""
    _, curve, settings, settlement = _curve()

    for n, unit in [(5, "Years"), (10, "Years")]:
        swap = MakeOis(
            P(n, unit),
            Estr(curve, settings),
            settings,
            forward_start=P(0, "Days"),
            effective_date=settlement,
            nominal=100.0,
            payment_lag=PAYMENT_LAG,
            discounting_term_structure=curve,
            averaging_method=RateAveraging.Compound,
        ).build()
        assert abs(swap.npv()) < 1.0e-6, f"{n} {unit} par OIS NPV {swap.npv()}"


def test_estr_forecasts_a_fixing_off_its_forwarding_curve():
    """Smoke test for Estr.fixing: forwarding off a flat 1% Actual/365 curve, the
    overnight forecast for a future date is close to 1%."""
    today = Date(5, 2, 2009)
    settings = Settings()
    settings.set_evaluation_date(today)

    curve = FlatForward(today, 0.01, DayCounter.actual365_fixed())
    estr = Estr(curve, settings)
    fixing = estr.fixing(Date(1, 6, 2009), False)
    assert fixing == pytest.approx(0.01, abs=1.0e-3)
