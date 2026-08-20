"""The year-on-year inflation swap facade (#849), struck at the rate its own
curve publishes.

A flat year-on-year curve at R forecasts R for every coupon, so a swap struck at
R pays the same amount on both legs on the same dates and is worth nothing. That
makes the fixture a discriminator rather than a tautology: it is the *forecast*
path - the quoted index reading the linked curve at each coupon's fixing date -
that has to land on R for the two legs to cancel, and a mis-wired schedule, lag
or day counter shows up as a non-zero NPV.

The index here is the quoted form, which is also what exercises the constructor
spelling region and currency out as their component fields. The ratio form and
a bootstrapped curve are the reprice oracle's job
(test_yoy_inflation_reprice.py).
"""

# itofin library
from itofin import Settings
from itofin.indexes import CpiInterpolationType, YoYInflationIndex
from itofin.instruments import SwapType, YearOnYearInflationSwap
from itofin.pricingengines import DiscountingSwapEngine
from itofin.termstructures import FlatForward, InterpolatedYoYInflationCurve
from itofin.time import BusinessDayConvention, Calendar, Date, DateGeneration, DayCounter, Frequency, Period, Schedule

TODAY = Date(13, 8, 2007)
CURVE_BASE = Date(1, 7, 2007)
CURVE_END = Date(1, 1, 2040)
MATURITY = Date(13, 8, 2012)
FLAT_YOY = 0.03
LAG = Period(2, "Months")
NOMINAL = 1_000_000.0
EPS = 1e-6
RATE_EPS = 1e-12


def _settings() -> Settings:
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    return settings


def _index(settings: Settings) -> YoYInflationIndex:
    """A quoted year-on-year index carrying UK RPI's own metadata, so its
    monthly frequency and one-month availability lag match the curve below."""
    return YoYInflationIndex(
        "YY_RPI",
        "UK",
        "GB",
        False,
        Frequency.Monthly,
        Period(1, "Months"),
        "British pound sterling",
        "GBP",
        826,
        "£",
        "p",
        100,
        settings,
    )


def _flat_curve() -> InterpolatedYoYInflationCurve:
    """Two nodes at the same rate: linear interpolation between them is that
    rate everywhere, and the base date precedes the reference date as every
    inflation curve's does."""
    return InterpolatedYoYInflationCurve(
        TODAY,
        [CURVE_BASE, CURVE_END],
        [FLAT_YOY, FLAT_YOY],
        Frequency.Monthly,
        DayCounter.thirty360_bond_basis(),
    )


def _a_swap(settings: Settings, fixed_rate: float) -> YearOnYearInflationSwap:
    """One schedule serves both legs, as the Rust oracle's does: unadjusted
    annual accrual dates, with the payment convention applied afterwards."""
    schedule = Schedule(
        TODAY,
        MATURITY,
        Frequency.Annual,
        Calendar.united_kingdom(),
        BusinessDayConvention.Unadjusted,
        DateGeneration.Backward,
    )
    day_counter = DayCounter.thirty360_bond_basis()
    index = _index(settings)
    index.link_to(_flat_curve())
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
    swap.set_engine(
        DiscountingSwapEngine(
            FlatForward(TODAY, 0.05, DayCounter.actual360()), settings
        )
    )
    return swap


def test_the_quoted_index_carries_the_metadata_it_was_given():
    """The quoted constructor's region and currency fields reach the core: the
    name is the region name and the family name joined, and the index is not a
    ratio one, so it has no underlying to defer to."""
    index = _index(_settings())

    assert index.name() == "UK YY_RPI"
    assert not index.ratio()
    assert index.underlying_index() is None


def test_the_curve_publishes_the_base_rate_it_was_seeded_with():
    """base_rate is the year-on-year divergence from the zero base, which defers
    it; the first node's rate is what the curve keeps."""
    curve = _flat_curve()

    assert abs(curve.base_rate() - FLAT_YOY) < RATE_EPS
    assert curve.base_date() == CURVE_BASE


def test_a_swap_struck_at_the_flat_curve_rate_is_worth_nothing():
    """Both legs pay the same amount on the same dates, so the NPV vanishes and
    the fair rate comes back to the rate the curve publishes. The leg NPVs are
    signed, so they add up to the swap NPV."""
    settings = _settings()
    swap = _a_swap(settings, FLAT_YOY)

    npv = swap.npv()
    print(f"|NPV| = {abs(npv):.3e}, fair rate = {swap.fair_rate()}")
    assert abs(npv) < EPS
    assert abs(swap.fair_rate() - FLAT_YOY) < EPS
    assert abs(swap.fixed_leg_npv() + swap.yoy_leg_npv() - npv) < EPS


def test_an_off_market_swap_prices_away_from_zero_and_reports_a_fair_spread():
    """The guard on the test above: a swap struck fifty basis points off the
    curve is emphatically not worth nothing, and its fair rate is still the
    curve's own - the fair values are recovered from the priced result rather
    than read off the strike."""
    settings = _settings()
    swap = _a_swap(settings, FLAT_YOY + 0.005)

    assert abs(swap.npv()) > 1.0
    assert abs(swap.fair_rate() - FLAT_YOY) < EPS
    assert abs(swap.fair_spread() - 0.005) < EPS
    assert swap.fixed_rate() == FLAT_YOY + 0.005
    assert swap.spread() == 0.0
