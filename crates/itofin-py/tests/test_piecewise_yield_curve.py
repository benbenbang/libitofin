"""PiecewiseYieldCurve bootstrap oracle (#529).

Port of the Rust curve-consistency oracle
(crates/libitofin/src/termstructures/yields/piecewiseyieldcurve.rs:263-506:
`log_linear_discount_consistency`, `linear_discount_consistency`, and
`bootstrap_is_lazy_and_reruns_on_quote_change`). The round-trip is
self-consistent: every instrument reprices its own input quote off the
bootstrapped curve, so there are no discount-factor literals; the pytest pins
the input quotes (DEPOSIT_DATA/SWAP_DATA, transcribed from
piecewiseyieldcurve.cpp). Tolerance 1e-9 (:322), checked with a bare
`abs(got - expected)` because `pytest.approx`'s default `rel=1e-6` would relax
1e-9 to ~4.5e-8.
"""

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import Euribor
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    DepositRateHelper,
    PiecewiseFlatForward,
    PiecewiseLinearForward,
    PiecewiseLinearZero,
    PiecewiseLogLinearDiscount,
    PiecewiseYieldCurve,
    SwapRateHelper,
)
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    DayCounter,
    Frequency,
)
from itofin.time import Period as P

# (n, unit, rate-in-percent), transcribed from piecewiseyieldcurve.cpp deposits.
DEPOSIT_DATA = [
    (1, "Weeks", 4.559),
    (1, "Months", 4.581),
    (2, "Months", 4.573),
    (3, "Months", 4.557),
    (6, "Months", 4.496),
    (9, "Months", 4.490),
]

# (n-years, rate-in-percent), transcribed from piecewiseyieldcurve.cpp swaps.
SWAP_DATA = [
    (1, 4.54),
    (2, 4.63),
    (3, 4.75),
    (4, 4.86),
    (5, 4.99),
    (6, 5.11),
    (7, 5.23),
    (8, 5.33),
    (9, 5.41),
    (10, 5.47),
    (12, 5.60),
    (15, 5.75),
    (20, 5.89),
    (25, 5.95),
    (30, 5.96),
]

TOLERANCE = 1.0e-9


def _fixture():
    """The Rust fixture head (piecewiseyieldcurve.rs:331-374): TARGET, evaluation
    date = adjust(15-Jun-2026), settlement = today + 2 business days. Returns the
    settings, calendar, today and settlement, plus the deposit and swap helpers
    (deposits over an empty forwarding handle; swaps floating off a fresh
    6M Euribor over an empty handle)."""
    calendar = Calendar.target()
    today = calendar.adjust(Date(15, 6, 2026), BusinessDayConvention.Following)
    settings = Settings()
    settings.set_evaluation_date(today)
    settlement = calendar.advance(
        today, 2, "Days", BusinessDayConvention.Following, False
    )

    deposits = []
    for n, unit, rate in DEPOSIT_DATA:
        index = Euribor(P(n, unit), None, settings)
        deposits.append(DepositRateHelper(SimpleQuote(rate / 100.0), index))

    swaps = []
    for n, rate in SWAP_DATA:
        euribor6m = Euribor(P(6, "Months"), None, settings)
        swaps.append(
            SwapRateHelper(
                SimpleQuote(rate / 100.0),
                P(n, "Years"),
                calendar,
                Frequency.Annual,
                BusinessDayConvention.Unadjusted,
                DayCounter.thirty360_bond_basis(),
                euribor6m,
            )
        )

    return settings, calendar, today, settlement, deposits, swaps


@pytest.mark.parametrize("interpolation", ["LogLinear", "Linear"])
def test_bootstrapped_curve_reprices_its_strip(interpolation):
    """The port of `testCurveConsistency<Discount, I, IterativeBootstrap>`,
    deposits + swaps, parametrized over both exposed interpolators
    (log_linear_discount_consistency :441, linear_discount_consistency :449).
    """
    settings, _, today, settlement, deposits, swaps = _fixture()
    instruments = deposits + swaps
    curve = PiecewiseYieldCurve(
        settlement, instruments, DayCounter.actual360(), interpolation
    )

    # Force the (lazy) bootstrap once, in-range, so the helpers are linked before
    # the swap arm reads implied_quote (piecewiseyieldcurve.rs:212 runs
    # calculate() then the range check, so t must be inside the curve span).
    curve.discount(0.5)

    # (a) Deposit arm - the discriminating check (:398-407): a FRESH index on the
    # bootstrapped curve reprices its own deposit rate. Independent of the helper,
    # so a wrong tenor/quote/date wiring fails here.
    for n, unit, rate in DEPOSIT_DATA:
        index = Euribor(P(n, unit), curve, settings)
        estimated = index.fixing(today, False)
        expected = rate / 100.0
        assert abs(estimated - expected) <= TOLERANCE, (
            f"{n} {unit} deposit: {estimated} vs {expected}"
        )

    # (b) Swap arm - a WEAK wiring smoke-test (bootstraphelper.rs:309,317):
    # quote_error = quote - implied_quote IS the bootstrap root, solved to ~1e-12,
    # so implied_quote re-asserts the solver's own residual and would still pass
    # with a wrongly-wired quote/tenor. The deposit arm is the independent oracle;
    # an independent swap reprice would need MakeVanillaSwap, which has no facade.
    for (n, rate), helper in zip(SWAP_DATA, swaps):
        estimated = helper.implied_quote()
        expected = rate / 100.0
        assert abs(estimated - expected) <= TOLERANCE, (
            f"{n}Y swap: {estimated} vs {expected}"
        )

    # (c) Shape - a structural check the solver cannot fake: discount factors are
    # strictly positive and strictly decreasing across the (increasing) pillar
    # dates.
    previous = 1.0
    for helper in instruments:
        df = curve.discount_date(helper.maturity_date())
        assert 0.0 < df < previous, f"non-decreasing/negative df {df} after {previous}"
        previous = df


def _lazy_curve():
    """A single-deposit curve (piecewiseyieldcurve.rs:456-489): 3M deposit at
    0.04557 over an empty handle. Returns the retained quote, the helper and the
    curve."""
    calendar = Calendar.target()
    today = calendar.adjust(Date(15, 6, 2026), BusinessDayConvention.Following)
    settings = Settings()
    settings.set_evaluation_date(today)
    settlement = calendar.advance(
        today, 2, "Days", BusinessDayConvention.Following, False
    )
    quote = SimpleQuote(0.04557)
    index = Euribor(P(3, "Months"), None, settings)
    helper = DepositRateHelper(quote, index)
    curve = PiecewiseYieldCurve(
        settlement, [helper], DayCounter.actual360(), "LogLinear"
    )
    return quote, helper, curve


def test_bootstrap_reruns_on_quote_change():
    """Laziness/re-bootstrap contract (:490-505): the first discount bootstraps
    to df1 in (0, 1); a quote bump to 0.06 invalidates the cache, and the next
    read re-bootstraps to a smaller df (a higher deposit rate discounts more).
    is_calculated is not observable from Python, so the observable df1/df2
    contract stands in for it."""
    quote, helper, curve = _lazy_curve()

    df1 = curve.discount_date(helper.maturity_date())
    assert 0.0 < df1 < 1.0

    quote.set_value(0.06)
    df2 = curve.discount_date(helper.maturity_date())
    assert df2 < df1, f"a higher deposit rate discounts more: {df2} vs {df1}"


def test_construction_does_not_bootstrap():
    """Construction lays out no nodes and runs no solver (piecewiseyieldcurve.rs:
    91-92): building a curve and never querying it must not raise, even though a
    later query might."""
    _quote, _helper, _curve = _lazy_curve()
    # No query; let it drop. Reaching here without an exception is the assertion.


def test_empty_helper_list_raises_at_construction():
    """The one thing the constructor rejects eagerly (:99): an empty helper
    list ("no bootstrap helpers given")."""
    _, _, _, settlement, _, _ = _fixture()
    with pytest.raises(ItofinError):
        PiecewiseYieldCurve(settlement, [], DayCounter.actual360(), "LogLinear")


def test_unknown_interpolation_raises():
    """An interpolation name outside {LogLinear, Linear, Cubic} is rejected at
    construction (the facade's string dispatch)."""
    _, _, _, settlement, deposits, _ = _fixture()
    with pytest.raises(ItofinError):
        PiecewiseYieldCurve(settlement, deposits, DayCounter.actual360(), "Spline")


def test_cubic_is_rejected_by_piecewise_with_reason():
    """Scope C guard (#547): PiecewiseYieldCurve<_, Cubic> COMPILES AND RUNS AND
    IS SILENTLY WRONG (Cubic is a global interpolator, but IterativeBootstrap is
    single-pass; on the merged mixed strip <ForwardRate, Cubic> mis-reprices by
    130bp while raising nothing). The named classes never offer Cubic and the
    string-dispatch arm rejects it with a reason that cites the unported
    convergence loop (#543) and the core-side guard (#552). A deposits-only strip
    would pass green even with Cubic (deposits read the curve only at their own
    pillar), so the rejection itself is the assertion here."""
    _, _, _, settlement, deposits, _ = _fixture()
    with pytest.raises(ItofinError) as excinfo:
        PiecewiseYieldCurve(settlement, deposits, DayCounter.actual360(), "Cubic")
    message = str(excinfo.value)
    assert "Cubic" in message
    assert "#543" in message
    assert "#552" in message


def test_bootstrap_failure_surfaces_at_query_not_construction():
    """A degenerate strip (two identical 3M deposits -> duplicate pillar dates,
    iterativebootstrap.rs:136-139) is accepted by the constructor and by
    max_date (which swallows the bootstrap error, piecewiseyieldcurve.rs:195-204)
    but raises from a discount query (:212-216)."""
    settings, _, _, settlement, _, _ = _fixture()
    index_a = Euribor(P(3, "Months"), None, settings)
    index_b = Euribor(P(3, "Months"), None, settings)
    helpers = [
        DepositRateHelper(SimpleQuote(0.04557), index_a),
        DepositRateHelper(SimpleQuote(0.04557), index_b),
    ]
    curve = PiecewiseYieldCurve(
        settlement, helpers, DayCounter.actual360(), "LogLinear"
    )

    # max_date swallows the failure and falls back to the reference date.
    curve.max_date()

    # a discount query surfaces it.
    with pytest.raises(ItofinError):
        curve.discount(0.5)


# --- named piecewise classes (#547) ------------------------------------------
#
# The four blessed (Traits, Interpolator) conventions from QuantLib-SWIG. They
# cannot be discriminated by any discount/zero/forward query: every valid combo
# reprices its strip to ~1e-13, and PiecewiseFlatForward is NUMERICALLY IDENTICAL
# to PiecewiseLogLinearDiscount under every query (log-linear in discount space
# IS piecewise-constant instantaneous forwards). Only the stored node data()
# separates the conventions, so the gate below pins data(), not repricing.

# Regression pins measured (via a Rust/Python probe, #547) on the merged mixed
# strip built by _fixture() - deposits (1W-9M) + swaps (1Y-30Y), 22 nodes. These
# are Rust-core wiring pins, NOT QuantLib oracle literals (there is no C++ number
# to hunt for); they match the reference measurement recorded in issue #547.
RATE_REFERENCE_NODE = 0.0455698048  # data[0]==data[1] for the three rate traits
DATA2_ZERO = 0.0457227821  # PiecewiseLinearZero    data[2]
DATA2_FORWARD_LINEAR = 0.0459688759  # PiecewiseLinearForward data[2]
DATA2_FORWARD_FLAT = 0.0457693404  # PiecewiseFlatForward   data[2]
PIN_TOLERANCE = 1.0e-9


def _named_curves():
    """All four named classes over the merged mixed strip (_fixture)."""
    _, _, _, settlement, deposits, swaps = _fixture()
    instruments = deposits + swaps
    dc = DayCounter.actual360()
    return {
        "log_linear_discount": PiecewiseLogLinearDiscount(settlement, instruments, dc),
        "linear_zero": PiecewiseLinearZero(settlement, instruments, dc),
        "linear_forward": PiecewiseLinearForward(settlement, instruments, dc),
        "flat_forward": PiecewiseFlatForward(settlement, instruments, dc),
    }, instruments


def test_named_classes_reprice_their_strip_wiring_only():
    """WIRING CHECK ONLY - does NOT discriminate the convention (every valid
    combo reprices to ~1e-13, so a mis-wired trait passes here green; the data()
    pin below is the real gate). A fresh index on each bootstrapped curve reprices
    every deposit rate to 1e-9, confirming each named ctor threaded its helpers
    into a curve that actually solved."""
    settings, _, today, _, _, _ = _fixture()
    curves, _ = _named_curves()
    for name, curve in curves.items():
        curve.discount(0.5)  # force the lazy bootstrap in-range
        for n, unit, rate in DEPOSIT_DATA:
            index = Euribor(P(n, unit), curve, settings)
            estimated = index.fixing(today, False)
            assert abs(estimated - rate / 100.0) <= TOLERANCE, (
                f"{name}: {n} {unit} deposit {estimated} vs {rate / 100.0}"
            )


def test_data_pin_discriminates_the_four_conventions():
    """THE GATE (#547). A reprice test cannot tell the four classes apart, so pin
    the stored node data() - the only surface the convention is visible on.

    - data[0] separates the storage SPACE: PiecewiseLogLinearDiscount stores
      discount factors (data[0] is the reference node's 1.0), the three rate
      conventions store rates (data[0] mirrors the first solved pillar ~0.04557).
    - data[0] == data[1] for the three rate conventions: update_guess mirrors the
      first solved rate into the reference node (core assertion
      piecewiseyieldcurve.rs:475/:491/:503; a missing mirror would leave data[0]
      at initial_value and still reprice green).
    - data[2] is MUTUALLY DISTINCT across the three rate conventions (zero rate vs
      the two forward rates at the second pillar differ) - this catches a
      copy-paste that wires e.g. PiecewiseLinearForward to <ZeroYield, Linear>,
      which the reprice check cannot. Separations are ~5e-5 to ~2e-4, orders of
      magnitude above the tolerance."""
    curves, _ = _named_curves()
    lld = curves["log_linear_discount"].data()
    zero = curves["linear_zero"].data()
    fwd_lin = curves["linear_forward"].data()
    fwd_flat = curves["flat_forward"].data()

    # storage space: discount factor 1.0 vs mirrored rate.
    assert lld[0] == 1.0
    for name, data in [("zero", zero), ("fwd_lin", fwd_lin), ("fwd_flat", fwd_flat)]:
        assert abs(data[0] - RATE_REFERENCE_NODE) <= PIN_TOLERANCE, name
        assert data[0] == data[1], f"{name}: reference node must mirror first pillar"

    # regression pins on the discriminating node.
    assert abs(zero[2] - DATA2_ZERO) <= PIN_TOLERANCE
    assert abs(fwd_lin[2] - DATA2_FORWARD_LINEAR) <= PIN_TOLERANCE
    assert abs(fwd_flat[2] - DATA2_FORWARD_FLAT) <= PIN_TOLERANCE

    # mutual distinctness - the real anti-miswiring assertion.
    margin = 1.0e-6
    assert abs(zero[2] - fwd_lin[2]) > margin
    assert abs(zero[2] - fwd_flat[2]) > margin
    assert abs(fwd_lin[2] - fwd_flat[2]) > margin


def test_flat_forward_equals_log_linear_discount_curve_but_data_differs():
    """The trap that forces the data() gate: PiecewiseFlatForward and
    PiecewiseLogLinearDiscount are the SAME CURVE (log-linear in discount space is
    piecewise-constant instantaneous forwards), so no discount / zero / forward
    query can EVER separate them - their discounts agree to ~1e-13 at every query
    point. Only the stored data() tell them apart, because they store in different
    spaces: discount factors (data[0] == 1.0) vs forward rates (data[0] ~0.04557).
    This is why queries are structurally unable to discriminate the conventions."""
    curves, _ = _named_curves()
    lld = curves["log_linear_discount"]
    ff = curves["flat_forward"]

    for t in [0.1, 0.35, 0.9, 2.0, 5.0]:
        assert abs(lld.discount(t) - ff.discount(t)) < 1.0e-13, (
            f"identical curves must agree at t={t}"
        )

    # yet the stored data lives in different spaces.
    assert lld.data()[0] == 1.0
    assert abs(ff.data()[0] - RATE_REFERENCE_NODE) <= PIN_TOLERANCE
    assert lld.data()[0] != ff.data()[0]


def test_off_pillar_discounts_distinct_except_the_identity_pair():
    """Wiring regression pin (#547, optional extra): off a pillar the four curves'
    discount(0.9) are mutually distinct EXCEPT the LogLinearDiscount/FlatForward
    pair, which is numerically identical (see the identity test). Separations are
    1e-4 to 6e-4 - a Rust-core wiring pin, not a QuantLib oracle number."""
    curves, _ = _named_curves()
    d = {name: curve.discount(0.9) for name, curve in curves.items()}

    assert abs(d["log_linear_discount"] - d["flat_forward"]) < 1.0e-13
    distinct = [d["log_linear_discount"], d["linear_zero"], d["linear_forward"]]
    for i in range(len(distinct)):
        for j in range(i + 1, len(distinct)):
            assert abs(distinct[i] - distinct[j]) > 1.0e-6


def test_named_classes_expose_node_dates_and_data():
    """dates() and data() read the concrete curve the erased handle discards, and
    both trigger the lazy bootstrap. One node per helper plus the reference node
    (a prerequisite the later node-count tickets rely on)."""
    curves, instruments = _named_curves()
    expected = len(instruments) + 1
    for name, curve in curves.items():
        assert len(curve.dates()) == expected, name
        assert len(curve.data()) == expected, name
