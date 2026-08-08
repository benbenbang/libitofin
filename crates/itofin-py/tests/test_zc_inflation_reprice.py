"""The Inflation-in-Python milestone (#752): fourteen quoted zero-coupon
inflation swaps bootstrap a curve and are then repriced to nothing off it.

This mirrors the Rust oracle's phase 1, `piecewisezeroinflationcurve.rs:779`
(itself `test-suite/inflation.cpp` `testZeroTermStructure` `:400-434`). The
swaps repriced here are rebuilt standalone on the *real* 5 % nominal curve, not
the helpers' own zero-strike swaps on their flat 0 % one, so the check is an
absolute one rather than a self-consistent round trip: it discriminates the
bootstrap's convergence, the base-date node placement, the fixing-period
quantization and the forecast formula at once, at the C++ tolerance of 1e-7.

Fixture. It is 13 August 2007. UK RPI carries the thirty-one monthly figures
published from January 2005, so the curve's base date is the July 2007 period
and every forecast compounds off its 207.3. Fourteen swap rates are quoted out
to 2057 under a three-month observation lag, flat CPI observation, UK
settlement calendar and Thirty360 BondBasis throughout.

Phase 3 (#813), the seasonality rerun, follows below: the same fourteen swaps
still reprice to nothing once a twelve-factor monthly correction is installed on
the bootstrapped curve, and the forecast the swaps reach it through demonstrably
moves.

Omitted visibly: phase 2 (`:913`), where the index forecasting off the
bootstrapped curve is checked against the curve's own compounded zero rate, is
left on the Rust side. It exercises no facade this file does not already build,
and the ticket scopes it out.

What the guards do and do not cover. Thirty-one hand-typed floats are the
largest silent-failure surface here, and a green milestone does not vindicate
them: only two are load-bearing for phase 1 - May 2007, which the swaps' base
observation reads from history, and July 2007, which the forecast compounds off
- and both are read back explicitly below. The remaining twenty-nine are
covered by count alone.
"""

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import CpiInterpolationType, ZeroInflationIndex
from itofin.instruments import SwapType, ZeroCouponInflationSwap
from itofin.pricingengines import DiscountingSwapEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    FlatForward,
    MultiplicativePriceSeasonality,
    PiecewiseZeroInflationCurve,
    ZeroCouponInflationSwapHelper,
)
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    DayCounter,
    Frequency,
    Period,
)

TODAY = Date(13, 8, 2007)
CURVE_BASE = Date(1, 7, 2007)
FIRST_OBSERVATION = Date(13, 5, 2008)
LAG = Period(3, "Months")
NOMINAL = 1_000_000.0
NOMINAL_RATE = 0.05
EPS = 1e-7
BASIS_POINT = 1e-4
FIXING_TOLERANCE = 1e-12

FIX_DATA = [
    189.9, 189.9, 189.6, 190.5, 191.6, 192.0, 192.2, 192.2, 192.6, 193.1,
    193.3, 193.6, 194.1, 193.4, 194.2, 195.0, 196.5, 197.7, 198.5, 198.5,
    199.2, 200.1, 200.4, 201.1, 202.7, 201.6, 203.1, 204.4, 205.4, 206.2,
    207.3,
]

ZC_DATA = [
    (Date(13, 8, 2008), 2.93),
    (Date(13, 8, 2009), 2.95),
    (Date(13, 8, 2010), 2.965),
    (Date(15, 8, 2011), 2.98),
    (Date(13, 8, 2012), 3.0),
    (Date(13, 8, 2014), 3.06),
    (Date(13, 8, 2017), 3.175),
    (Date(13, 8, 2019), 3.243),
    (Date(15, 8, 2022), 3.293),
    (Date(14, 8, 2027), 3.338),
    (Date(13, 8, 2032), 3.348),
    (Date(15, 8, 2037), 3.348),
    (Date(13, 8, 2047), 3.308),
    (Date(13, 8, 2057), 3.228),
]

LOAD_BEARING_FIXINGS = [(Date(1, 5, 2007), 205.4), (CURVE_BASE, 207.3)]

SEASONALITY_FACTORS = [
    1.003245, 1.000000, 0.999715, 1.000495, 1.000929, 0.998687,
    0.995949, 0.994682, 0.995949, 1.000519, 1.003705, 1.004186,
]

SEASONALITY_ANCHOR = Date(31, 1, 2007)
SEASONALITY_PROBE = Date(1, 8, 2012)
FORECAST_RAW = 239.95839754779345
FORECAST_CORRECTED = 238.4450543710035
FORECAST_MATCH = 1e-10
RATE_MOVE = 1e-4


def _fixing_date(i: int) -> Date:
    """The i-th monthly fixing date, walked from 1 January 2005.

    A monthly index is filed under the first of its month and the schedule is
    never adjusted, so the walk is plain month arithmetic; `Date.__add__` takes
    days, not a Period.
    """
    return Date(1, i % 12 + 1, 2005 + i // 12)


def _settings() -> Settings:
    """One Settings object serves the index, every helper, every swap and every
    engine: a swap and its engine resolving their dates against different
    evaluation dates would price silently wrong rather than raise."""
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    return settings


def _index(settings: Settings) -> ZeroInflationIndex:
    index = ZeroInflationIndex.uk_rpi(settings)
    for i, fixing in enumerate(FIX_DATA):
        index.add_fixing(_fixing_date(i), fixing)
    return index


def _nominal_curve() -> FlatForward:
    """The 5 % discount curve the standalone swaps price on. The three-argument
    facade is Continuous compounding at Annual frequency, which is what the
    Rust fixture passes explicitly."""
    return FlatForward(TODAY, NOMINAL_RATE, DayCounter.actual360())


def _helpers(
    settings: Settings, index: ZeroInflationIndex
) -> list[ZeroCouponInflationSwapHelper]:
    return [
        ZeroCouponInflationSwapHelper(
            SimpleQuote(rate / 100.0),
            LAG,
            maturity,
            Calendar.united_kingdom(),
            BusinessDayConvention.ModifiedFollowing,
            DayCounter.thirty360_bond_basis(),
            index,
            CpiInterpolationType.Flat,
            settings,
        )
        for maturity, rate in ZC_DATA
    ]


def _bootstrapped(
    settings: Settings, index: ZeroInflationIndex
) -> PiecewiseZeroInflationCurve:
    """The curve, with the caller's index relinked onto it.

    The helpers forecast through copies of the index on handles of their own,
    so the link matters only to the standalone swaps built afterwards - which
    is exactly what the milestone prices.
    """
    curve = PiecewiseZeroInflationCurve(
        TODAY,
        index.last_fixing_date(),
        Frequency.Monthly,
        DayCounter.thirty360_bond_basis(),
        _helpers(settings, index),
    )
    index.link_to(curve)
    return curve


def _a_swap(
    settings: Settings,
    index: ZeroInflationIndex,
    maturity: Date,
    fixed_rate: float,
) -> ZeroCouponInflationSwap:
    """A quoted swap rebuilt standalone on the real nominal curve.

    Each gets its own engine, as the Rust fixture does: an engine carries the
    arguments and results of the contract it last priced.
    """
    swap = ZeroCouponInflationSwap(
        SwapType.Payer,
        NOMINAL,
        TODAY,
        maturity,
        Calendar.united_kingdom(),
        BusinessDayConvention.ModifiedFollowing,
        DayCounter.thirty360_bond_basis(),
        fixed_rate,
        index,
        LAG,
        CpiInterpolationType.Flat,
        None,
        None,
        settings,
    )
    swap.set_engine(DiscountingSwapEngine(_nominal_curve(), settings))
    return swap


def _a_seasonality(factors: list[float] = None) -> MultiplicativePriceSeasonality:
    """The twelve monthly factors phase 3 installs (`inflation.cpp:469-483`).

    The anchor is 31 January of the year the curve's base period ends in
    (`:467-468`). The base date is 1 July 2007 and a monthly inflation period
    runs to the month's end, so that year is 2007; the Rust fixture derives it
    with `inflation_period`, which is not exposed here, and
    test_the_transcribed_fixture_is_the_one_the_oracle_runs pins the base date
    the derivation starts from.

    The anchor is not discriminated by the numbers below: the factor lookup
    wraps modulo the factor count, so shifting a stationary twelve-factor set by
    a whole year picks the very same factors. It is pinned by derivation alone.
    """
    return MultiplicativePriceSeasonality(
        SEASONALITY_ANCHOR,
        Frequency.Monthly,
        SEASONALITY_FACTORS if factors is None else factors,
    )


def test_the_transcribed_fixture_is_the_one_the_oracle_runs():
    """The transcription guards. The counts pin the two hand-typed tables, the
    base date pins where the last figure's period falls, and the two figures the
    milestone actually reads are checked by value - a wrong one elsewhere in
    FIX_DATA would move nothing phase 1 can see."""
    assert len(FIX_DATA) == 31
    assert len(ZC_DATA) == 14

    settings = _settings()
    index = _index(settings)
    assert index.last_fixing_date() == CURVE_BASE

    for date, expected in LOAD_BEARING_FIXINGS:
        assert abs(index.fixing(date) - expected) <= FIXING_TOLERANCE


def test_the_first_helper_observes_three_months_before_its_maturity():
    """Checked before the bootstrap, as the oracle checks it: the observation
    is the raw 13 May 2008 the helper's contract prices at, not the 1 May 2008
    its curve node is rounded to."""
    settings = _settings()
    helpers = _helpers(settings, _index(settings))

    assert helpers[0].inflation_fixing_date() == FIRST_OBSERVATION


def test_the_first_node_is_the_base_date_at_a_negative_time():
    """Node zero sits on the base date rather than on the reference date at
    time zero, so the first time is negative. Both reads go through dates() and
    times(), which propagate a bootstrap failure; a range-checked query would
    report the evaluation date as the maximum and hide it."""
    settings = _settings()
    curve = _bootstrapped(settings, _index(settings))

    assert curve.dates()[0] == curve.base_date()
    assert curve.dates()[0] == CURVE_BASE
    assert curve.times()[0] < 0.0


def test_the_bootstrapped_curve_reprices_the_quoted_swaps_to_zero():
    """The milestone. Every quoted swap, rebuilt standalone and discounted on
    the 5 % nominal curve, comes back worth nothing off the bootstrapped
    inflation curve; and the analytic fixed-leg BPS matches a repriced
    one-basis-point bump of the same contract.

    The margins are printed rather than only asserted: the distance from 1e-7
    is the report, and the Rust oracle over the identical fixture measures
    2.62e-9 and 2.39e-9.
    """
    settings = _settings()
    index = _index(settings)
    _bootstrapped(settings, index)

    npv_by_maturity = {}
    worst_npv = 0.0
    worst_bps = 0.0
    for maturity, rate in ZC_DATA:
        swap = _a_swap(settings, index, maturity, rate / 100.0)
        npv = swap.npv()
        npv_by_maturity[str(maturity)] = npv
        worst_npv = max(worst_npv, abs(npv))

        bumped = _a_swap(settings, index, maturity, rate / 100.0 + BASIS_POINT)
        expected = bumped.fixed_leg_npv() - swap.fixed_leg_npv()
        worst_bps = max(worst_bps, abs(swap.fixed_leg_bps() - expected))

    print(f"worst |NPV| = {worst_npv:.3e}, worst fixed-leg BPS error = {worst_bps:.3e}")
    print(f"NPV by maturity = {npv_by_maturity}")
    assert worst_npv < EPS
    assert worst_bps < EPS


def test_a_seasonality_moves_the_forecast_and_the_curve_reprices_the_swaps_again():
    """Phase 3. Reprice-to-zero alone would be a false pass here: if the
    correction were stored but never folded into the published rate, the
    re-bootstrap would hit the very same nodes off the very same quotes and
    every swap would still come back worth nothing.

    So the discriminator comes first. The index's own forecast - the path every
    helper and every swap reaches the curve through - is required to move, and
    to move to the level the Rust oracle measures over this identical fixture
    (`piecewisezeroinflationcurve.rs:878`, which prints 239.95839754779345 ->
    238.4450543710035 at this same date). Clearing the correction has to put the
    raw forecast back. The reprice loop then covers the other half: with the
    correction live, only a bootstrap that re-solved against it returns to zero.
    """
    settings = _settings()
    index = _index(settings)
    curve = _bootstrapped(settings, index)

    assert not curve.has_seasonality()
    raw_forecast = index.fixing(SEASONALITY_PROBE, True)
    raw_rate = curve.zero_rate_date(SEASONALITY_PROBE)
    assert abs(raw_forecast - FORECAST_RAW) < FORECAST_MATCH

    curve.set_seasonality(_a_seasonality())
    assert curve.has_seasonality()
    corrected_forecast = index.fixing(SEASONALITY_PROBE, True)
    corrected_rate = curve.zero_rate_date(SEASONALITY_PROBE)
    print(f"forecast at {SEASONALITY_PROBE}: {raw_forecast} -> {corrected_forecast}")
    print(f"zero rate at {SEASONALITY_PROBE}: {raw_rate} -> {corrected_rate}")
    assert abs(corrected_forecast - FORECAST_CORRECTED) < FORECAST_MATCH
    assert abs(corrected_forecast - raw_forecast) > 1e-3
    assert abs(corrected_rate - raw_rate) > RATE_MOVE

    worst_npv = 0.0
    for maturity, rate in ZC_DATA:
        swap = _a_swap(settings, index, maturity, rate / 100.0)
        worst_npv = max(worst_npv, abs(swap.npv()))
    print(f"worst |NPV| under seasonality = {worst_npv:.3e}")
    assert worst_npv < EPS

    curve.set_seasonality(None)
    assert not curve.has_seasonality()
    assert abs(index.fixing(SEASONALITY_PROBE, True) - raw_forecast) < FORECAST_MATCH
    assert abs(curve.zero_rate_date(SEASONALITY_PROBE) - raw_rate) < FORECAST_MATCH


def test_a_multi_year_seasonality_is_rejected_by_the_consistency_gate():
    """The gate reports the correction's own verdict, and a twenty-four-factor
    set is the multi-year branch the core defers (#807).

    It runs on its own fixture because the failure leaves the curve in a state
    the reprice test must not inherit: the store happens before the gate, as
    C++'s does, so the rejected correction stays installed - which
    has_seasonality() reports as True below, deliberately - while the
    notification that would invalidate the bootstrap never fires. Reading the
    curve from here would fold a correction onto nodes that were never re-solved
    against it, so the correction is cleared before the test ends.
    """
    settings = _settings()
    index = _index(settings)
    curve = _bootstrapped(settings, index)

    with pytest.raises(ItofinError) as raised:
        curve.set_seasonality(_a_seasonality(SEASONALITY_FACTORS * 2))
    assert "#807" in str(raised.value)

    assert curve.has_seasonality(), "the store precedes the gate, as C++'s does"
    curve.set_seasonality(None)
    assert not curve.has_seasonality()
