"""OIS rate-helper bootstrap - facing the overnight strip (#551).

Mirrors the merged core oracle `ois_bootstrap_reprices_the_quotes`
(crates/libitofin/src/termstructures/yields/ratehelpers.rs:1677-1753): an ESTR
discounting curve bootstrapped purely from OISRateHelpers reprices every OIS
quote it was built from.

HONEST LIMITATION - convergence/wiring gate, not a discriminating oracle. The
core test's discriminating arm reprices with MakeOis + fair_rate()
(ratehelpers.rs:1731-1746); neither MakeOis nor an OvernightIndexedSwap is faced
in itofin-py yet, so the Python acceptance is the helper-level reprice:
abs(implied_quote - quote_value) is the bootstrap's OWN root
(bootstraphelper.rs:317) driven to ~1e-12, so it re-asserts the solver residual
and would still pass a mis-wired quote. The structural arms (node count, strictly
decreasing discount factors) carry what discrimination there is. A discriminating
OIS oracle needs a MakeOis/OvernightIndexedSwap facade - a recommended follow-up.
"""

import pytest

from itofin import Settings
from itofin.indexes import Estr
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
    helper list and the curve."""
    today = Date(5, 2, 2009)
    settings = Settings()
    settings.set_evaluation_date(today)
    _ = Calendar.target()

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
    return helpers, curve


def test_ois_bootstrap_reprices_the_quotes():
    """Port of `ois_bootstrap_reprices_the_quotes`. Three arms:

    1. STRUCTURAL PIN: one curve node per helper plus the reference node.
       `dates()` also forces the lazy bootstrap, so the reprice arm below reads a
       solved curve. This arm cannot be faked by the solver.
    2. WIRING/CONVERGENCE GATE (not a discriminating oracle, see module docstring):
       abs(implied_quote - quote_value) IS the bootstrap's own root, solved to
       ~1e-12, so it re-asserts the solver residual within 1e-8.
    3. SHAPE: discount factors strictly positive and strictly decreasing across
       the (increasing) helper maturity dates - a structural property the solver
       cannot fake.
    """
    helpers, curve = _curve()

    # Arm 1 - structural pin (forces the bootstrap).
    nodes = curve.dates()
    assert len(nodes) == len(helpers) + 1, (
        "one curve node per helper plus the reference"
    )

    # Arm 2 - wiring/convergence gate (re-asserts the solver root, see docstring).
    worst = 0.0
    for helper in helpers:
        implied = helper.implied_quote()
        quote = helper.quote_value()
        error = abs(implied - quote)
        worst = max(worst, error)
        assert error <= TOLERANCE, (
            f"OIS reprice: implied {implied} vs quote {quote} (err {error})"
        )
    assert worst <= TOLERANCE, f"worst OIS reprice error {worst}"

    # Arm 3 - shape: strictly positive, strictly decreasing discount factors.
    previous = 1.0
    for helper in helpers:
        df = curve.discount_date(helper.maturity_date())
        assert 0.0 < df < previous, f"non-decreasing/negative df {df} after {previous}"
        previous = df


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
