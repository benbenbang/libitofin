"""Oracle for the optionlet-stripping facades (issue #623): the discriminating
round-trip for the whole cap/floor volatility vertical.

The fixture mirrors the core's own round-trip test,
``flat_term_vol_round_trips_through_the_stripped_optionlets``
(``strippedoptionletadapter.rs:334``), which is the Rust port of
``optionletstripper.cpp`` ``testFlatTermVolatilityStripping1`` (``:489-548``):
evaluation date 28-October-2013, a flat 4% Actual/365F curve, Euribor6M on it,
and a FLAT 18% cap/floor term-vol surface over 1Y..10Y x 1%..10%.

The surface must be built by ``moving``, not by the pinned-reference
constructor. ``StrippedOptionletAdapter`` reads its settlement days back off the
term-vol surface (``strippedoptionletadapter.rs:87`` through
``optionletstripper.rs:241``), and a pinned-reference term structure has none,
so the pinned forms fail the adapter with "settlement days not provided for this
instance". The curve may stay pinned: the round-trip identity is curve-agnostic,
since both engines share it.

Four arms:

A. THE ROUND-TRIP, at 2.5e-8. For each of the 100 (tenor, strike) grid points,
   the same cap is priced twice: once on a BlackCapFloorEngine over the stripped
   adapter, once on an engine over a flat 18% quote. The two paths could hardly
   differ more - the stripped path runs surface -> per-caplet bootstrap ->
   optionlet grid -> linear interpolation in strike and time -> Black, while the
   flat path is one constant surface - so agreement is a statement that the
   strip round-trips, not a tautology.

   The tolerance is 2.5e-8, taken from the C++ ``vars.tolerance``
   (``optionletstripper.cpp:78``) and carried by the core's own test, NOT
   widened. It is that tight for a structural reason: the stripper's cap lengths
   step by the 6M index tenor (``optionletstripper.rs:128-151``), so each
   stripped optionlet corresponds to exactly one caplet and the price
   differencing telescopes exactly; and every cap here is priced at a grid tenor
   and a grid strike, which are interpolation NODES, where the adapter's linear
   interpolation is exact. What is left is the implied-volatility solve, whose
   accuracy is 1e-6 in standard-deviation units. Do not widen this: a larger
   error means the Python path diverges from the core's, not that the bound is
   wrong.

   Each grid point builds a fresh cap per engine. An Instrument caches its NPV,
   so reusing one cap would let the second engine serve the first one's number.

B. The switch strike at 1e-12 against the mean of the at-the-money caplet rates,
   mirroring ``optionletstripper1.rs:497-505``. The mean is computed here from
   ``atm_optionlet_rates()`` rather than pinned as a literal, so the arm is
   self-contained.

C. VolatilityType.Normal is deferred (#440/#577) and is rejected AT THE STRIP,
   not at construction: ``OptionletStripper1.__init__`` succeeds, and the error
   surfaces from the first call that needs the grid (``optionletstripper1.rs:
   170-175``, mirrored by the core test at ``:510``). Both the stripper query and
   the adapter constructor must raise, since the adapter's constructor strips.

D. The pinned-reference surface is rejected by the adapter, which is why arm A
   uses ``moving``. This pins the constraint that made the moving constructors
   part of this pass rather than a deferral.

One shared Settings drives everything: the instruments and the engines must
agree on the evaluation date or the NPVs are silently wrong.
"""

# pypi/conda library
import pytest

# itofin library
from itofin import ItofinError, Settings
from itofin.indexes import Euribor
from itofin.instruments import CapFloor, CapFloorType
from itofin.pricingengines import BlackCapFloorEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    CapFloorTermVolSurface,
    FlatForward,
    OptionletStripper1,
    StrippedOptionletAdapter,
    VolatilityType,
)
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Period

EVAL = Date(28, 10, 2013)
CURVE_RATE = 0.04
FLAT_VOL = 0.18

OPTION_TENORS = [Period(n, "Years") for n in range(1, 11)]
STRIKES = [j / 100.0 for j in range(1, 11)]
VOLS = [[FLAT_VOL] * len(STRIKES) for _ in OPTION_TENORS]

SPOT = Period(0, "Days")
TOLERANCE = 2.5e-8
SWITCH_STRIKE_TOLERANCE = 1e-12

SETTINGS = Settings()
SETTINGS.set_evaluation_date(EVAL)


def _curve():
    return FlatForward(EVAL, CURVE_RATE, DayCounter.actual365_fixed())


def _moving_surface():
    """The flat term-vol surface with a floating reference date, the form the
    stripper and the adapter need."""
    return CapFloorTermVolSurface.moving(
        0,
        Calendar.target(),
        BusinessDayConvention.Following,
        OPTION_TENORS,
        STRIKES,
        VOLS,
        DayCounter.actual365_fixed(),
        SETTINGS,
    )


def _pinned_surface():
    return CapFloorTermVolSurface(
        EVAL,
        Calendar.target(),
        BusinessDayConvention.Following,
        OPTION_TENORS,
        STRIKES,
        VOLS,
        DayCounter.actual365_fixed(),
    )


def _stripper(
    curve,
    surface=None,
    volatility_type=VolatilityType.ShiftedLognormal,
    optionlet_frequency=None,
):
    surface = _moving_surface() if surface is None else surface
    return OptionletStripper1(
        surface,
        Euribor.six_months(curve, SETTINGS),
        volatility_type,
        optionlet_frequency=optionlet_frequency,
    )


def _cap(tenor, strike, curve):
    """A fresh cap, so no cached NPV survives an engine swap."""
    return CapFloor(
        CapFloorType.Cap,
        tenor,
        Euribor.six_months(curve, SETTINGS),
        strike,
        SPOT,
        SETTINGS,
    )


def test_a_flat_term_vol_surface_round_trips_through_the_stripped_optionlets():
    curve = _curve()
    adapter = StrippedOptionletAdapter(_stripper(curve), SETTINGS)
    adapter.enable_extrapolation()

    stripped_engine = BlackCapFloorEngine(adapter, curve, None)
    flat_engine = BlackCapFloorEngine.with_flat_vol(
        curve, SimpleQuote(FLAT_VOL), DayCounter.actual365_fixed(), 0.0, SETTINGS
    )

    worst = 0.0
    worst_at = None
    for tenor in OPTION_TENORS:
        for strike in STRIKES:
            stripped = _cap(tenor, strike, curve)
            stripped.set_black_engine(stripped_engine)
            price_stripped = stripped.npv()

            flat = _cap(tenor, strike, curve)
            flat.set_black_engine(flat_engine)
            price_flat = flat.npv()

            error = abs(price_stripped - price_flat)
            if error > worst:
                worst, worst_at = error, (str(tenor), strike)
            assert error < TOLERANCE, (
                f"tenor {tenor} strike {strike}: stripped {price_stripped!r} vs "
                f"flat {price_flat!r}, error {error!r} > {TOLERANCE!r}"
            )

    print(f"\nworst round-trip |npv diff| = {worst!r} at {worst_at}")
    assert worst > 0.0, (
        "the anti-tautology guard: a stripper that echoed the flat input straight "
        "through would feed both engines the identical vol, forward, strike, time "
        "and discount, and the two NPVs would agree bit-for-bit. A non-zero worst "
        "error is what proves the stripped caplet vols really differ from the flat "
        "term vol, so arm A is a round-trip and not a circular identity."
    )


def test_the_switch_strike_is_the_mean_atm_caplet_rate():
    stripper = _stripper(_curve())
    rates = stripper.atm_optionlet_rates()
    assert len(rates) > 1
    expected = sum(rates) / len(rates)
    assert stripper.switch_strike() == pytest.approx(
        expected, abs=SWITCH_STRIKE_TOLERANCE
    )


def test_the_optionlet_frequency_overrides_the_index_tenor_as_the_caplet_step():
    """The caplet grid is built by stepping the index tenor across the surface
    (``optionletstripper.rs:128-151``), so a 1Y override on a 6M index halves the
    number of caplets. This is what pins that the argument reaches the core
    rather than being dropped by the facade."""
    curve = _curve()
    by_index_tenor = len(_stripper(curve).atm_optionlet_rates())
    by_override = len(
        _stripper(curve, optionlet_frequency=Period(1, "Years")).atm_optionlet_rates()
    )
    assert by_override == pytest.approx(by_index_tenor / 2, abs=1)
    assert by_override < by_index_tenor


def test_a_normal_volatility_type_is_rejected_at_the_strip_not_at_construction():
    curve = _curve()
    stripper = _stripper(curve, volatility_type=VolatilityType.Normal)
    with pytest.raises(ItofinError):
        stripper.switch_strike()
    with pytest.raises(ItofinError):
        StrippedOptionletAdapter(_stripper(curve, volatility_type=VolatilityType.Normal), SETTINGS)


def test_a_pinned_reference_surface_is_rejected_by_the_adapter():
    stripper = _stripper(_curve(), surface=_pinned_surface())
    with pytest.raises(ItofinError):
        StrippedOptionletAdapter(stripper, SETTINGS)


def test_the_adapter_serves_the_flat_input_volatility_back():
    curve = _curve()
    adapter = StrippedOptionletAdapter(_stripper(curve), SETTINGS)
    for tenor in (Period(2, "Years"), Period(5, "Years")):
        for strike in (0.02, 0.05):
            assert adapter.volatility(tenor, strike) == pytest.approx(
                FLAT_VOL, abs=0.02
            )
