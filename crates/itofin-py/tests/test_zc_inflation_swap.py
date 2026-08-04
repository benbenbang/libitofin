"""ZeroCouponInflationSwap priced without a bootstrap (#750).

The swap is built over a DIRECTLY-built InterpolatedZeroInflationCurve (#749)
linked into a UK RPI index (#748) through the new ZeroInflationIndex.link_to,
and discounted by a flat nominal curve. A directly-built curve does not reprice
its own swaps to zero, so every number below is a pin taken from a throwaway
Rust probe over the identical fixture, not a milestone the swap has to hit.

What the pins are for: the 14-argument constructor order, the forecast reaching
the index through the curve link, and the engine reaching the swap. The core
numerics are not under test here - crates/libitofin/src/instruments/
zerocouponinflationswap.rs pins those against hand-derived amounts, including
the raw-vs-adjusted year fraction that this fixture cannot see.

Fixture. It is 13 August 2007. Two RPI figures are on record: May 2007, which
the swap's own base observation (13 Aug 2007 less the three-month lag) reads
from history, and July 2007, the curve's base date, which the forecast
machinery compounds off (inflationindex.rs:479-498). Both sit at or before the
publication horizon - 13 Aug less RPI's one-month availability lag puts it in
July 2007 - so neither is itself forecast. The maturity observation, 13 May
2008, is well past it and so is forecast off the curve; 1 May 2008 falls
strictly between the 1 Jan and 1 Jul 2008 nodes, so it reads an interpolated
rate rather than a node outright.
"""

import pytest
from itofin import ItofinError, Settings
from itofin.indexes import CpiInterpolationType, ZeroInflationIndex
from itofin.instruments import SwapType, ZeroCouponInflationSwap
from itofin.pricingengines import DiscountingSwapEngine
from itofin.termstructures import FlatForward, InterpolatedZeroInflationCurve
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
BASE_OBSERVATION = Date(1, 5, 2007)
CURVE_DATES = [CURVE_BASE, Date(1, 9, 2007), Date(1, 1, 2008), Date(1, 7, 2008), Date(1, 7, 2009)]
CURVE_RATES = [0.02, 0.022, 0.025, 0.027, 0.030]
MAY_2007_FIXING = 205.0
JULY_2007_FIXING = 207.3
NOMINAL = 1_000_000.0
FIXED_RATE = 0.025
NOMINAL_RATE = 0.05

FAIR_RATE = 0.03336195814008680
NPV = -7953.05109561582139577
FIXED_LEG_NPV = 23777.47820061785387225
INFLATION_LEG_NPV = -31730.52929623367526801
FIXED_LEG_BPS = 95.10991280246126678
TOLERANCE = 1e-7


def _settings() -> Settings:
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    return settings


def _curve() -> InterpolatedZeroInflationCurve:
    return InterpolatedZeroInflationCurve(
        TODAY,
        list(CURVE_DATES),
        list(CURVE_RATES),
        Frequency.Monthly,
        DayCounter.thirty360_bond_basis(),
    )


def _index(settings: Settings) -> ZeroInflationIndex:
    """UK RPI carrying the two figures the fixture reads from history. No curve
    is linked yet: link_to is what the tests below exercise."""
    index = ZeroInflationIndex.uk_rpi(settings)
    index.add_fixing(BASE_OBSERVATION, MAY_2007_FIXING)
    index.add_fixing(CURVE_BASE, JULY_2007_FIXING)
    return index


def _swap(settings: Settings, index: ZeroInflationIndex) -> ZeroCouponInflationSwap:
    """A payer, so the swap pays inflation and receives fixed. Both optional
    inflation-leg conventions are left None and so resolve to the fixed-leg
    ones."""
    return ZeroCouponInflationSwap(
        SwapType.Payer,
        NOMINAL,
        TODAY,
        MATURITY,
        Calendar.united_kingdom(),
        BusinessDayConvention.ModifiedFollowing,
        DayCounter.thirty360_bond_basis(),
        FIXED_RATE,
        index,
        Period(3, "Months"),
        CpiInterpolationType.Flat,
        None,
        None,
        settings,
    )


def _engine(settings: Settings) -> DiscountingSwapEngine:
    nominal = FlatForward(TODAY, NOMINAL_RATE, DayCounter.actual365_fixed())
    return DiscountingSwapEngine(nominal, settings)


def _priced_swap() -> ZeroCouponInflationSwap:
    settings = _settings()
    index = _index(settings)
    index.link_to(_curve())
    swap = _swap(settings, index)
    swap.set_engine(_engine(settings))
    return swap


def test_the_observation_date_lags_the_maturity():
    """The one datum read off the indexed cash flow. 13 May 2008 is the raw
    maturity less the three-month lag, unsnapped to its inflation period, and
    obs_date is the same call on the swap (zerocouponinflationswap.rs:315-317)
    under the flow-level name."""
    settings = _settings()
    swap = _swap(settings, _index(settings))

    assert swap.inflation_fixing_date() == Date(13, 5, 2008)
    assert swap.obs_date() == swap.inflation_fixing_date()


def test_the_maturity_round_trips_raw():
    """13 August 2008 is a Wednesday, so the raw maturity and both adjusted
    payment dates coincide and this cannot tell the override from the base
    Swap's span over the legs. The Saturday fixture that does is in the core
    (`the_raw_dates_override_the_bases_span_of_the_legs`); here it is an input
    round-trip."""
    settings = _settings()

    assert _swap(settings, _index(settings)).maturity_date() == MATURITY


def test_an_unlinked_index_cannot_price_and_link_to_fixes_it():
    """The pin that makes link_to's wiring observable. The index's handle
    starts empty, so the maturity forecast - and everything downstream of it -
    raises until a curve is linked. Nothing about the swap changes in between:
    the same object prices afterwards.

    It does not show that a relink refreshes an already-priced swap. The failed
    calculate() left the instrument uncalculated, so the second call simply
    recomputes; whether a notification reaches a swap that priced successfully
    is the separate #387 question."""
    settings = _settings()
    index = _index(settings)
    swap = _swap(settings, index)
    swap.set_engine(_engine(settings))

    with pytest.raises(ItofinError, match="empty Handle"):
        swap.fair_rate()
    with pytest.raises(ItofinError, match="empty Handle"):
        swap.npv()

    index.link_to(_curve())

    assert abs(swap.fair_rate() - FAIR_RATE) <= TOLERANCE
    assert abs(swap.npv() - NPV) <= TOLERANCE


def test_the_swap_prices_to_the_probed_values():
    """The probe pins, leg by leg. The sum identity is what would catch the two
    leg accessors wired to the wrong index: a swapped pair leaves the total
    unchanged, but the legs differ in both sign and magnitude - a payer
    receives the fixed 23777 and pays the inflation 31730."""
    swap = _priced_swap()

    assert abs(swap.fixed_leg_npv() - FIXED_LEG_NPV) <= TOLERANCE
    assert abs(swap.inflation_leg_npv() - INFLATION_LEG_NPV) <= TOLERANCE
    assert abs(swap.npv() - NPV) <= TOLERANCE
    assert abs(swap.fixed_leg_npv() + swap.inflation_leg_npv() - swap.npv()) <= TOLERANCE

    assert swap.npv() != 0.0


def test_the_fixed_leg_bps_reaches_the_engines_discount_factor():
    """fixed_leg_bps is computed in closed form but still needs the engine, for
    the discount factor at the fixed leg's end date. It is called here with no
    prior npv(), which is the load-bearing part: the core's end_discounts
    prices on demand (swap.rs:307-315)."""
    swap = _priced_swap()

    assert abs(swap.fixed_leg_bps() - FIXED_LEG_BPS) <= TOLERANCE


def test_fair_rate_needs_no_engine_but_npv_does():
    """fair_rate reads the indexed flow rather than any priced result
    (zerocouponinflationswap.rs:364), so it answers on an engine-less swap
    while npv reports the null engine.

    Under Thirty360 BondBasis this maturity is exactly one year out, so the
    de-compounding is the identity and the fair rate equals the raw index
    growth. That makes the literal readable but blind to the year fraction;
    the core pins that separately."""
    settings = _settings()
    index = _index(settings)
    index.link_to(_curve())
    swap = _swap(settings, index)

    assert abs(swap.fair_rate() - FAIR_RATE) <= TOLERANCE
    with pytest.raises(ItofinError, match="null pricing engine"):
        swap.npv()


def test_a_lag_the_index_cannot_observe_through_is_rejected():
    """Construction is fallible. Under Flat the bar is RPI's own one-month
    availability lag; under Linear the interpolation eats a further publication
    period, so a one-month lag clears the first and fails the second
    (zerocouponinflationswap.rs:156-176)."""
    settings = _settings()
    month = Period(1, "Months")

    with pytest.raises(ItofinError, match="fixings that do not yet exist"):
        _build_with_lag(settings, Period(0, "Months"), CpiInterpolationType.Flat)
    with pytest.raises(ItofinError, match="inconsistency between swap observation lag"):
        _build_with_lag(settings, month, CpiInterpolationType.Linear)

    assert _build_with_lag(settings, month, CpiInterpolationType.Flat) is not None


def _build_with_lag(
    settings: Settings, lag: Period, interpolation: CpiInterpolationType
) -> ZeroCouponInflationSwap:
    return ZeroCouponInflationSwap(
        SwapType.Payer,
        NOMINAL,
        TODAY,
        MATURITY,
        Calendar.united_kingdom(),
        BusinessDayConvention.ModifiedFollowing,
        DayCounter.thirty360_bond_basis(),
        FIXED_RATE,
        _index(settings),
        lag,
        interpolation,
        None,
        None,
        settings,
    )
