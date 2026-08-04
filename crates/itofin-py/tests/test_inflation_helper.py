"""ZeroCouponInflationSwapHelper and PiecewiseZeroInflationCurve, structurally
(#751).

The numeric milestone - fourteen quoted swaps repriced to zero off the
bootstrapped curve - is #752. What is checked here is the wiring the milestone
would fail on for reasons it could not report: the helper's dates, the visible
rejection of the interpolated observation branch, and that the bootstrapped
curve lays node zero on the base date and reaches Python through *both* halves
of its class chain.

Fixture. It is 13 August 2007. UK RPI carries the four monthly figures the Rust
core fixture carries (piecewisezeroinflationcurve.rs:345-356), so the curve's
base date is July 2007 and every forecast compounds off its 207.3. One swap is
quoted, at 3 % to 13 August 2008 under a three-month observation lag. The
caller's index is deliberately never linked to a curve: the helper prices
through a copy of it on a handle of its own, so a bootstrap that needed the
caller's link would fail here rather than silently read the wrong curve.

The two dates the helper reports are not the same date and the fixture is
chosen so they cannot be confused. 13 August 2008 less three months is 13 May
2008, which is what the contract observes; the helper rounds that to 1 May 2008,
the first day of its monthly inflation period, and puts its curve node there.
"""

import pytest
from itofin import ItofinError, Settings
from itofin.indexes import CpiInterpolationType, ZeroInflationIndex
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    PiecewiseZeroInflationCurve,
    ZeroCouponInflationSwapHelper,
    ZeroInflationHelper,
    ZeroInflationTermStructure,
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
MATURITY = Date(13, 8, 2008)
CURVE_BASE = Date(1, 7, 2007)
OBSERVATION = Date(13, 5, 2008)
PILLAR = Date(1, 5, 2008)
LAG = Period(3, "Months")
QUOTE = 0.03
FIXINGS = [
    (Date(1, 4, 2007), 204.4),
    (Date(1, 5, 2007), 205.4),
    (Date(1, 6, 2007), 206.2),
    (CURVE_BASE, 207.3),
]
PLAUSIBLE_RATES = (0.01, 0.05)
TRAITS_SEED = 0.02
TOLERANCE = 1e-12


def _settings() -> Settings:
    """The evaluation date is set before anything else is built: the helper's
    swap starts at it and is constructed inside the helper's own constructor,
    which reports an unset date rather than deferring it."""
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    return settings


def _index(settings: Settings) -> ZeroInflationIndex:
    index = ZeroInflationIndex.uk_rpi(settings)
    for date, fixing in FIXINGS:
        index.add_fixing(date, fixing)
    return index


def _helper(
    settings: Settings,
    index: ZeroInflationIndex,
    interpolation: CpiInterpolationType = CpiInterpolationType.Flat,
) -> ZeroCouponInflationSwapHelper:
    return ZeroCouponInflationSwapHelper(
        SimpleQuote(QUOTE),
        LAG,
        MATURITY,
        Calendar.united_kingdom(),
        BusinessDayConvention.ModifiedFollowing,
        DayCounter.thirty360_bond_basis(),
        index,
        interpolation,
        settings,
    )


def _curve(settings: Settings, index: ZeroInflationIndex) -> PiecewiseZeroInflationCurve:
    return PiecewiseZeroInflationCurve(
        TODAY,
        index.last_fixing_date(),
        Frequency.Monthly,
        DayCounter.thirty360_bond_basis(),
        [_helper(settings, index)],
    )


def test_the_helper_rounds_its_pillar_off_the_observation_it_prices_at():
    """The two dates apart. inflation_fixing_date reads the cached contract's
    indexed flow, so it reports the raw 13 May 2008 the helper actually prices
    at; pillar_date is the first day of that observation's monthly period, and
    latest_date coincides with it."""
    settings = _settings()
    helper = _helper(settings, _index(settings))

    assert isinstance(helper, ZeroInflationHelper)
    assert helper.inflation_fixing_date() == OBSERVATION
    assert helper.pillar_date() == PILLAR
    assert helper.latest_date() == PILLAR


def test_the_caller_index_needs_no_curve_link():
    """The helper prices through a copy of the index on a handle of its own, so
    the caller's index is still unlinked here and the bootstrap runs anyway.
    A facade that handed the caller's index straight through would raise the
    empty-handle error off the first read."""
    settings = _settings()
    index = _index(settings)
    with pytest.raises(ItofinError, match="empty Handle"):
        index.fixing(OBSERVATION)

    assert _curve(settings, index).calculate() is None


def test_the_interpolated_observation_branch_is_rejected():
    """Omitted visibly: Linear's date and pillar logic is a documented deferral
    of the port, so a caller asking for it is told rather than quietly given the
    flat dates."""
    settings = _settings()

    with pytest.raises(ItofinError, match="not ported"):
        _helper(settings, _index(settings), CpiInterpolationType.Linear)


def test_the_first_node_is_the_base_date_at_a_negative_time():
    """The one structural difference from every other piecewise curve: node zero
    sits on the base date, which precedes the reference date, rather than on the
    reference date at time zero.

    The time is pinned exactly rather than by sign, so it discriminates the day
    counter too: Thirty360 BondBasis from 13 August 2007 back to 1 July 2007 is
    30 * (7 - 8) + (1 - 13) = -42, over 360."""
    settings = _settings()
    curve = _curve(settings, _index(settings))

    assert curve.dates()[0] == CURVE_BASE
    assert curve.times()[0] == pytest.approx(-42.0 / 360.0, abs=TOLERANCE)
    assert curve.dates() == [CURVE_BASE, PILLAR]
    assert [date for date, _ in curve.nodes()] == curve.dates()


def test_the_bootstrapped_curve_answers_through_its_base_class():
    """Every inspector above reads the retained concrete curve, so none of them
    would notice an erased handle wired to the wrong object - or to nothing.
    The reads here go through ZeroInflationTermStructure instead, and answer the
    same solved rate.

    That rate is bounded rather than pinned; the number itself is #752's. What
    the bound is for is the solver: the traits seed every node at 0.02 before
    the first iteration, so a rate that merely sits in a plausible band proves
    nothing on its own and the distance from the seed is asserted separately."""
    settings = _settings()
    curve = _curve(settings, _index(settings))

    assert isinstance(curve, ZeroInflationTermStructure)
    assert curve.base_date() == CURVE_BASE
    assert curve.frequency() == Frequency.Monthly

    low, high = PLAUSIBLE_RATES
    assert low < curve.zero_rate_date(PILLAR) < high
    assert curve.zero_rate_date(PILLAR) == pytest.approx(curve.nodes()[1][1], abs=TOLERANCE)
    assert abs(curve.zero_rate_date(PILLAR) - TRAITS_SEED) > 1e-3


def test_an_empty_helper_list_is_rejected():
    with pytest.raises(ItofinError, match="no bootstrap helpers"):
        PiecewiseZeroInflationCurve(
            TODAY,
            CURVE_BASE,
            Frequency.Monthly,
            DayCounter.thirty360_bond_basis(),
            [],
        )
