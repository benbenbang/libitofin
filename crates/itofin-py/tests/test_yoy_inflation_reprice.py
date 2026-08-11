"""The year-on-year inflation milestone (#849): fifteen quoted year-on-year
inflation swaps bootstrap a curve, and the fourteen past the base pillar are
then repriced to nothing off it.

This mirrors the Rust oracle `piecewiseyoyinflationcurve.rs:807-858` (itself
`test-suite/inflation.cpp` `testYYTermStructure` `:1198-1224`). The swaps
repriced here are rebuilt standalone on the real 5 % nominal curve rather than
read off the helpers, so nothing the bootstrap cached can carry the result: only
the curve does. It discriminates the bootstrap's convergence, the base-date node
placement, the fixing-period quantization and the year-on-year forecast at once.

Fixture. It is 13 August 2007. UK RPI carries the thirty-one monthly figures
published from January 2005, so the curve's base date is the July 2007 period.
The year-on-year index is the *ratio* form over that RPI, which owns no history
of its own and divides two RPI figures a year apart. Fifteen swap rates are
quoted out to 2037 under a two-month observation lag, flat CPI observation, UK
settlement calendar and Thirty360 BondBasis throughout.

Two frequencies live in this fixture and must not be conflated. The *curve* is
Monthly, because UK RPI publishes monthly and the curve's frequency is what
quantizes its nodes and its base date. The *swap schedules* are Annual, because
year-on-year coupons pay yearly. Passing either one where the other belongs
moves every number.

Omitted visibly: seasonality on a year-on-year curve, which the zero side covers
(test_zc_inflation_reprice.py) through the same base-class setter this curve
inherits.
"""

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import CpiInterpolationType, YoYInflationIndex, ZeroInflationIndex
from itofin.instruments import SwapType, YearOnYearInflationSwap
from itofin.pricingengines import DiscountingSwapEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    FlatForward,
    PiecewiseYoYInflationCurve,
    YearOnYearInflationSwapHelper,
)
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    DateGeneration,
    DayCounter,
    Frequency,
    Period,
    Schedule,
)

TODAY = Calendar.united_kingdom().adjust(
    Date(13, 8, 2007), BusinessDayConvention.Following
)
CURVE_BASE = Date(1, 7, 2007)
LAG = Period(2, "Months")
NOMINAL = 1_000_000.0
NOMINAL_RATE = 0.05
EPS = 1e-6

FIX_DATA = [
    189.9, 189.9, 189.6, 190.5, 191.6, 192.0, 192.2, 192.2, 192.6, 193.1,
    193.3, 193.6, 194.1, 193.4, 194.2, 195.0, 196.5, 197.7, 198.5, 198.5,
    199.2, 200.1, 200.4, 201.1, 202.7, 201.6, 203.1, 204.4, 205.4, 206.2,
    207.3,
]

YY_DATA = [
    (Date(13, 8, 2008), 2.95),
    (Date(13, 8, 2009), 2.95),
    (Date(13, 8, 2010), 2.93),
    (Date(15, 8, 2011), 2.955),
    (Date(13, 8, 2012), 2.945),
    (Date(13, 8, 2013), 2.985),
    (Date(13, 8, 2014), 3.01),
    (Date(13, 8, 2015), 3.035),
    (Date(13, 8, 2016), 3.055),
    (Date(13, 8, 2017), 3.075),
    (Date(13, 8, 2019), 3.105),
    (Date(15, 8, 2022), 3.135),
    (Date(13, 8, 2027), 3.155),
    (Date(13, 8, 2032), 3.145),
    (Date(13, 8, 2037), 3.145),
]


def _fixing_date(i: int) -> Date:
    """The i-th monthly fixing date, walked from 1 January 2005.

    A monthly index is filed under the first of its month and the schedule is
    never adjusted, so the walk is plain month arithmetic.
    """
    return Date(1, i % 12 + 1, 2005 + i // 12)


def _settings() -> Settings:
    """One Settings object serves the index, every helper, every swap and every
    engine: a swap and its engine resolving their dates against different
    evaluation dates would price silently wrong rather than raise."""
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    return settings


def _rpi(settings: Settings) -> ZeroInflationIndex:
    """The published UK RPI. The fixings go on the *zero* index: the ratio
    year-on-year index files none of its own and reads this history."""
    rpi = ZeroInflationIndex.uk_rpi(settings)
    for i, fixing in enumerate(FIX_DATA):
        rpi.add_fixing(_fixing_date(i), fixing)
    return rpi


def _nominal_curve() -> FlatForward:
    """The 5 % discount curve the helpers and the standalone swaps both run on.
    The three-argument facade is Continuous compounding at Annual frequency,
    which is what the Rust fixture passes explicitly."""
    return FlatForward(Date(13, 8, 2007), NOMINAL_RATE, DayCounter.actual360())


def _helpers(
    settings: Settings,
    index: YoYInflationIndex,
    nominal: FlatForward,
    interpolation: CpiInterpolationType = CpiInterpolationType.Flat,
) -> list[YearOnYearInflationSwapHelper]:
    return [
        YearOnYearInflationSwapHelper(
            SimpleQuote(rate / 100.0),
            LAG,
            maturity,
            Calendar.united_kingdom(),
            BusinessDayConvention.ModifiedFollowing,
            DayCounter.thirty360_bond_basis(),
            index,
            interpolation,
            nominal,
            settings,
        )
        for maturity, rate in YY_DATA
    ]


def _bootstrapped(
    settings: Settings, index: YoYInflationIndex, nominal: FlatForward
) -> PiecewiseYoYInflationCurve:
    """The curve, with the caller's index relinked onto it.

    The curve frequency is Monthly, matching the RPI the ratio index reads; the
    swap schedules below are Annual, which is a different thing entirely. Node
    zero is seeded with the first quote and kept rather than solved, which is
    why the reprice loop skips that pillar.
    """
    curve = PiecewiseYoYInflationCurve(
        TODAY,
        CURVE_BASE,
        YY_DATA[0][1] / 100.0,
        Frequency.Monthly,
        DayCounter.thirty360_bond_basis(),
        _helpers(settings, index, nominal),
    )
    index.link_to(curve)
    return curve


def _a_swap(
    settings: Settings,
    index: YoYInflationIndex,
    nominal: FlatForward,
    maturity: Date,
    fixed_rate: float,
) -> YearOnYearInflationSwap:
    """A quoted swap rebuilt standalone on the real nominal curve, running from
    that curve's reference date. One schedule serves both legs, as the Rust
    oracle's does. Each swap gets its own engine: an engine carries the
    arguments and results of the contract it last priced."""
    schedule = Schedule(
        nominal.reference_date(),
        maturity,
        Frequency.Annual,
        Calendar.united_kingdom(),
        BusinessDayConvention.Unadjusted,
        DateGeneration.Backward,
    )
    day_counter = DayCounter.thirty360_bond_basis()
    swap = YearOnYearInflationSwap(
        SwapType.Payer,
        NOMINAL,
        schedule,
        fixed_rate,
        day_counter,
        schedule,
        index,
        LAG,
        CpiInterpolationType.Flat,
        0.0,
        day_counter,
        Calendar.united_kingdom(),
        BusinessDayConvention.ModifiedFollowing,
        settings,
    )
    swap.set_engine(DiscountingSwapEngine(nominal, settings))
    return swap


def test_the_transcribed_fixture_is_the_one_the_oracle_runs():
    """The transcription guards. The counts pin the two hand-typed tables, and
    the base date pins where the last RPI figure's period falls - it is the
    curve's node zero, so a month-walk landing in the wrong period would move
    every rate the swaps forecast."""
    assert len(FIX_DATA) == 31
    assert len(YY_DATA) == 15

    settings = _settings()
    rpi = _rpi(settings)
    assert rpi.last_fixing_date() == CURVE_BASE

    index = YoYInflationIndex.from_underlying(rpi)
    assert index.ratio()
    assert index.name() == "UK YYR_RPI"
    assert index.underlying_index() is rpi, "the ratio index hands back the very object it was built over, not a fresh facade whose relinkable handle the core index would never see"


def test_the_first_node_is_the_base_date():
    """Node zero sits on the base date rather than on the reference date, so the
    first time is negative. Both reads go through dates() and times(), which
    propagate a bootstrap failure rather than hiding it."""
    settings = _settings()
    nominal = _nominal_curve()
    index = YoYInflationIndex.from_underlying(_rpi(settings))
    curve = _bootstrapped(settings, index, nominal)

    assert curve.dates()[0] == curve.base_date()
    assert curve.dates()[0] == CURVE_BASE
    assert curve.times()[0] < 0.0


def test_an_interpolated_helper_is_refused():
    """CpiInterpolationType.Linear is refused outright by the core helper: the
    interpolated branch of the C++ constructor is deferred with the rest of
    CPI::Linear (#847), and refusing is how that stays visible rather than
    silently pricing as a flat one."""
    settings = _settings()
    index = YoYInflationIndex.from_underlying(_rpi(settings))

    with pytest.raises(ItofinError) as raised:
        _helpers(settings, index, _nominal_curve(), CpiInterpolationType.Linear)
    assert "not ported yet" in str(raised.value)


def test_the_bootstrapped_curve_reprices_the_quoted_swaps_to_zero():
    """The milestone. Every quoted swap past the base pillar, rebuilt standalone
    and discounted on the 5 % nominal curve, comes back worth nothing off the
    bootstrapped year-on-year curve.

    Row 0 is skipped: its rate seeds node zero rather than being solved for, so
    the curve was never fitted to a swap at that maturity. The margin is printed
    rather than only asserted - the distance from 1e-6 is the report.
    """
    settings = _settings()
    nominal = _nominal_curve()
    index = YoYInflationIndex.from_underlying(_rpi(settings))
    _bootstrapped(settings, index, nominal)

    npv_by_maturity = {}
    worst_npv = 0.0
    for row in range(1, len(YY_DATA)):
        maturity, rate = YY_DATA[row]
        npv = _a_swap(settings, index, nominal, maturity, rate / 100.0).npv()
        npv_by_maturity[str(maturity)] = npv
        worst_npv = max(worst_npv, abs(npv))

    print(f"worst |NPV| = {worst_npv:.3e}")
    print(f"NPV by maturity = {npv_by_maturity}")
    assert worst_npv < EPS
