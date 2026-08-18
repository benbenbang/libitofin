"""Oracle for the cap/floor + optionlet-vol + Black-engine facades (issue #621).

The bindings oracle is the wrapped Rust numbers, and this pass is SMOKE and
STRUCTURAL by design. The core's ``blackcapfloorengine.rs`` cached NPV is built
over a hand-rolled ``IborLeg`` (notional 100, spot caplet kept) that ``MakeCapFloor``
cannot reproduce - it uses a unit nominal and drops the spot caplet - and there is
no ``IborLeg`` facade, so that literal is unreachable from Python. The
discriminating numeric oracle for this vertical is the optionlet-stripper
round-trip in #623, which is construction-agnostic.

What is pinned here:

A. A 5Y Euribor6M cap at 4% on a flat 5% curve builds, refuses to price with no
   engine, and prices finite and positive once a Black engine is attached; the
   matching floor at 6% prices positive too. The strike reaches the engine: the
   4% cap is worth more than a 6% cap on the same leg.
B. The two engine constructors agree bit-for-bit. ``with_flat_vol`` wraps the
   quote in a MOVING constant optionlet surface with 0 settlement days on a null
   calendar, so its reference date IS the evaluation date - the same surface arm
   B builds explicitly with a fixed reference at that date. The two routes run
   the identical float sequence, so this is an equality, not a tolerance. This is
   what makes the smoke arms construction-pinning without a cached literal.
C. The engine's displacement guard (``blackcapfloorengine.rs:75-82``) is reachable
   from Python: a displacement that differs from the surface's own is an error.
D. ``CapFloorType.Collar`` exists (the year-on-year inflation cap/floor builds
   one, #859) but is refused here: ``MakeCapFloor`` builds caps and floors only.

Every arm builds its own ``CapFloor``. An ``Instrument`` caches its NPV, so
reusing one cap across two engines would let arm B pass on a stale number. One
shared ``Settings`` drives everything: the instrument and the engine must agree
on the evaluation date or the NPV is silently wrong.
"""

import math

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import Euribor
from itofin.instruments import CapFloor, CapFloorType
from itofin.pricingengines import BlackCapFloorEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    ConstantOptionletVolatility,
    FlatForward,
    VolatilityType,
)
from itofin.time import BusinessDayConvention, Calendar, DayCounter, Date, Period

EVAL = Date(15, 1, 2026)

CURVE_RATE = 0.05
CAP_STRIKE = 0.04
FLOOR_STRIKE = 0.06
OUT_OF_THE_MONEY_STRIKE = 0.06

VOL = 0.20
TENOR = Period(5, "Years")
SPOT = Period(0, "Days")

SETTINGS = Settings()
SETTINGS.set_evaluation_date(EVAL)


def _curve():
    return FlatForward(EVAL, CURVE_RATE, DayCounter.actual365_fixed())


def _surface(displacement=0.0):
    """The fixed-reference twin of the moving surface ``with_flat_vol`` builds."""
    return ConstantOptionletVolatility(
        EVAL,
        Calendar.null_calendar(),
        BusinessDayConvention.Following,
        VOL,
        DayCounter.actual365_fixed(),
        VolatilityType.ShiftedLognormal,
        displacement,
    )


def _cap_floor(cap_floor_type, strike):
    """A fresh 5Y instrument on the one shared ``Settings``."""
    curve = _curve()
    index = Euribor.six_months(curve, SETTINGS)
    return curve, CapFloor(cap_floor_type, TENOR, index, strike, SPOT, SETTINGS)


def test_a_cap_builds_and_carries_its_strike():
    _, cap = _cap_floor(CapFloorType.Cap, CAP_STRIKE)
    assert cap.coupon_count() > 0
    assert cap.cap_rates() == [CAP_STRIKE] * cap.coupon_count()
    assert cap.floor_rates() == []


def test_a_floor_carries_its_strike_as_a_floor_rate():
    _, floor = _cap_floor(CapFloorType.Floor, FLOOR_STRIKE)
    assert floor.floor_rates() == [FLOOR_STRIKE] * floor.coupon_count()
    assert floor.cap_rates() == []


def test_pricing_without_an_engine_raises():
    _, cap = _cap_floor(CapFloorType.Cap, CAP_STRIKE)
    with pytest.raises(ItofinError):
        cap.npv()


def _npv_via_surface(cap_floor_type, strike):
    """Priced through the surface-handle engine constructor."""
    curve, instrument = _cap_floor(cap_floor_type, strike)
    instrument.set_black_engine(BlackCapFloorEngine(_surface(), curve, None))
    return instrument.npv()


def _npv_via_flat_vol(cap_floor_type, strike):
    """Priced through the flat-quote engine constructor, which builds the same
    constant surface internally."""
    curve, instrument = _cap_floor(cap_floor_type, strike)
    instrument.set_black_engine(
        BlackCapFloorEngine.with_flat_vol(
            curve, SimpleQuote(VOL), DayCounter.actual365_fixed(), 0.0, SETTINGS
        )
    )
    return instrument.npv()


def test_a_cap_prices_finite_and_positive():
    npv = _npv_via_surface(CapFloorType.Cap, CAP_STRIKE)
    print(f"\ncap@{CAP_STRIKE} npv = {npv!r}")
    assert math.isfinite(npv)
    assert npv > 0.0


def test_a_floor_prices_finite_and_positive():
    npv = _npv_via_surface(CapFloorType.Floor, FLOOR_STRIKE)
    print(f"\nfloor@{FLOOR_STRIKE} npv = {npv!r}")
    assert math.isfinite(npv)
    assert npv > 0.0


def test_the_strike_reaches_the_engine():
    in_the_money = _npv_via_surface(CapFloorType.Cap, CAP_STRIKE)
    out_of_the_money = _npv_via_surface(CapFloorType.Cap, OUT_OF_THE_MONEY_STRIKE)
    assert out_of_the_money < in_the_money


def test_a_higher_volatility_raises_the_cap_price():
    """What makes the equality below non-degenerate: the surface is read, so the
    two routes agreeing is a statement about the surface they build, not about
    two vol-independent intrinsic values."""
    curve, cap = _cap_floor(CapFloorType.Cap, CAP_STRIKE)
    cap.set_black_engine(BlackCapFloorEngine(_surface(), curve, None))
    _, dearer = _cap_floor(CapFloorType.Cap, CAP_STRIKE)
    dearer.set_black_engine(
        BlackCapFloorEngine.with_flat_vol(
            curve, SimpleQuote(2.5 * VOL), DayCounter.actual365_fixed(), 0.0, SETTINGS
        )
    )
    assert dearer.npv() > cap.npv()


def test_the_two_engine_constructors_price_identically():
    npv_surface = _npv_via_surface(CapFloorType.Cap, CAP_STRIKE)
    npv_flat = _npv_via_flat_vol(CapFloorType.Cap, CAP_STRIKE)
    print(f"\nnpv_surface = {npv_surface!r}\nnpv_flat_vol = {npv_flat!r}")
    assert npv_surface == npv_flat, f"surface={npv_surface!r} flat={npv_flat!r}"


def test_a_displacement_differing_from_the_surface_raises():
    curve = _curve()
    assert BlackCapFloorEngine(_surface(), curve, 0.0).displacement() == 0.0
    with pytest.raises(ItofinError):
        BlackCapFloorEngine(_surface(), curve, 0.01)


def test_no_displacement_adopts_the_surfaces_own():
    """A shifted surface makes the two branches of the displacement match
    distinguishable: with a 0.0-shift surface, None and 0.0 are the same number."""
    curve = _curve()
    assert BlackCapFloorEngine(_surface(0.01), curve, None).displacement() == 0.01
    assert BlackCapFloorEngine(_surface(0.01), curve, 0.01).displacement() == 0.01
    with pytest.raises(ItofinError):
        BlackCapFloorEngine(_surface(0.01), curve, 0.0)


def test_a_normal_surface_is_rejected_by_the_black_engine():
    normal = ConstantOptionletVolatility(
        EVAL,
        Calendar.null_calendar(),
        BusinessDayConvention.Following,
        VOL,
        DayCounter.actual365_fixed(),
        VolatilityType.Normal,
        0.0,
    )
    with pytest.raises(ItofinError):
        BlackCapFloorEngine(normal, _curve(), None)


def test_constant_surface_returns_the_constructed_volatility():
    surface = _surface()
    for tenor in [1, 2, 5]:
        assert surface.volatility(Period(tenor, "Years"), CAP_STRIKE) == VOL
        assert surface.volatility_date(EVAL + 365 * tenor, CAP_STRIKE) == VOL
    assert surface.displacement() == 0.0


def test_constant_surface_black_variance_is_vol_squared_times_time():
    surface = _surface()
    one_year = Period(1, "Years")
    option_time = surface.black_variance(one_year, CAP_STRIKE) / (VOL * VOL)
    assert option_time == pytest.approx(1.0, abs=0.01)


def test_quote_backed_surface_tracks_its_quote():
    quote = SimpleQuote(VOL)
    surface = ConstantOptionletVolatility.with_quote(
        EVAL,
        Calendar.null_calendar(),
        BusinessDayConvention.Following,
        quote,
        DayCounter.actual365_fixed(),
        VolatilityType.ShiftedLognormal,
        0.0,
    )
    one_year = (Period(1, "Years"), CAP_STRIKE)
    assert surface.volatility(*one_year) == VOL
    quote.set_value(0.25)
    assert surface.volatility(*one_year) == 0.25


def test_a_collar_over_an_ibor_leg_is_refused():
    """``CapFloorType.Collar`` reaches Python for the year-on-year inflation
    cap/floor (#859), whose raw constructors take a coupon vector. This
    instrument is built through ``MakeCapFloor``, which carries a single strike
    and refuses a collar outright (``makecapfloor.rs:135``); the raw
    ``CapFloor::collar`` needs an ``IborLeg`` facade that does not exist yet
    (#626). So the enum value arrives here and the build does not."""
    assert hasattr(CapFloorType, "Cap")
    assert hasattr(CapFloorType, "Floor")
    assert hasattr(CapFloorType, "Collar")

    with pytest.raises(ItofinError) as raised:
        _cap_floor(CapFloorType.Collar, CAP_STRIKE)
    assert "not collars" in str(raised.value)
