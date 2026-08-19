"""Oracle for the cap/floor + optionlet-vol + Black-engine facades (#621, #626).

Two passes live here. The ``MakeCapFloor`` pass (#621) is SMOKE and STRUCTURAL:
that builder uses a unit nominal and drops the spot caplet, so it cannot
reproduce the core's cached literal and the arms below pin construction and
engine agreement instead.

The raw-leg pass (#626) at the end of this file IS the numeric oracle. The
core's ``blackcapfloorengine.rs`` cached NPV is built over a hand-rolled
``IborLeg`` - notional 100, spot caplet kept - which the ``IborLeg`` facade now
lets Python lay out, so ``CapFloor.cap`` / ``floor`` / ``collar`` reproduce that
literal through the raw constructors. See the section header there.

What is pinned by the MakeCapFloor pass:

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
D. ``CapFloorType.Collar`` exists but is refused by this constructor:
   ``MakeCapFloor`` builds caps and floors only. The collar is reachable through
   the raw-leg pass below.

What is pinned by the raw-leg pass:

E. The core's cached cap 6.87570026732 and floor 2.65812927959, to 1e-11, over
   the ``CommonVars`` fixture of ``blackcapfloorengine.rs:304-418``: it is
   14-Mar-2002, a flat 5% Actual360 continuously compounded curve references
   18-Mar-2002, Euribor 6M forecasts off it, and the leg runs 20 years
   semiannually on TARGET, ModifiedFollowing at both ends, generated forwards,
   on a notional of 100 with two fixing days and the index's own day counter.
   The engine is Black at a flat 20% vol. The par arm is the one pinned:
   ``using_at_par`` defaults true, so no flag is passed.
F. That the raw-leg path is what produced them. ``MakeCapFloor`` drops the spot
   caplet and uses a unit nominal, so those literals are unreachable through
   the pass above: hitting them proves the ``IborLeg`` facade laid every coupon
   out itself and that the raw constructor consumed exactly that leg. Each of
   the notional, the payment day counter and the fixing days moves the cap by
   far more than the tolerance, so all three are pinned by the literal.
G. The collar, as the cap less the floor, and with it the argument order:
   ``CapFloor.collar`` takes cap rates first, and swapping the lists builds a
   long-3%-cap short-7%-floor instrument nowhere near the pinned value.
H. That the payment adjustment reaches the coupons, which E cannot show - see
   that test's own docstring.

Every arm builds its own ``CapFloor``. An ``Instrument`` caches its NPV, so
reusing one cap across two engines would let arm B pass on a stale number, and
the raw-leg arms build a fresh engine per instrument as the core does.

Two ``Settings`` objects live here, deliberately. The MakeCapFloor arms resolve
against ``SETTINGS`` at 2026 and the raw-leg arms against ``ORACLE_SETTINGS`` at
14-Mar-2002; moving the shared one would let test ordering decide the other
pass's fate. Within each pass a single object drives the curve-backed index,
every instrument and every engine, or the leg and the optionlets date
differently with no error raised.
"""

import math

import pytest

from itofin import ItofinError, Settings
from itofin.cashflows import IborLeg
from itofin.indexes import Euribor
from itofin.instruments import CapFloor, CapFloorType
from itofin.pricingengines import BlackCapFloorEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    ConstantOptionletVolatility,
    FlatForward,
    VolatilityType,
)
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    DayCounter,
    Frequency,
    Period,
    Schedule,
)

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


def test_a_collar_is_refused_by_the_market_builder():
    """The refusal is a ``MakeCapFloor`` property, not a gap: that builder
    carries a single strike and rejects a collar outright
    (``makecapfloor.rs:135``). A collar over a floating leg is built through
    ``CapFloor.collar`` instead, which is pinned in the raw-leg pass below."""
    assert hasattr(CapFloorType, "Cap")
    assert hasattr(CapFloorType, "Floor")
    assert hasattr(CapFloorType, "Collar")

    with pytest.raises(ItofinError) as raised:
        _cap_floor(CapFloorType.Collar, CAP_STRIKE)
    assert "not collars" in str(raised.value)


ORACLE_EVAL = Date(14, 3, 2002)
ORACLE_REFERENCE = Date(18, 3, 2002)
ORACLE_RATE = 0.05
ORACLE_VOL = 0.20
ORACLE_NOTIONAL = 100.0
ORACLE_YEARS = 20

CACHED_CAP = 6.87570026732
CACHED_FLOOR = 2.65812927959
CACHED_COLLAR = CACHED_CAP - CACHED_FLOOR

CAP_RATE = 0.07
FLOOR_RATE = 0.03

ORACLE_SETTINGS = Settings()
ORACLE_SETTINGS.set_evaluation_date(ORACLE_EVAL)


def _oracle_curve():
    return FlatForward(ORACLE_REFERENCE, ORACLE_RATE, DayCounter.actual360())


def _oracle_leg(curve):
    """CommonVars::makeLeg: the 20Y semiannual leg the cached value is built
    over, on the index day counter, ModifiedFollowing, two fixing days.

    The end date is computed off the calendar rather than written out: a
    literal that differed from the adjusted value would shift the whole
    schedule and take the 1e-11 pins down with no useful message."""
    calendar = Calendar.target()
    modified_following = BusinessDayConvention.ModifiedFollowing
    index = Euribor.six_months(curve, ORACLE_SETTINGS)
    end = calendar.advance(
        ORACLE_REFERENCE, ORACLE_YEARS, "Years", modified_following, False
    )
    schedule = Schedule(
        ORACLE_REFERENCE,
        end,
        Frequency.Semiannual,
        calendar,
        modified_following,
        termination_convention=modified_following,
    )
    return (
        IborLeg(schedule, index)
        .with_notional(ORACLE_NOTIONAL)
        .with_payment_day_counter(index.day_counter())
        .with_payment_adjustment(modified_following)
        .with_fixing_days(2)
    )


def _oracle_engine(curve):
    """CommonVars::makeEngine. A fresh engine per instrument, as the core builds
    one per instrument: an Instrument caches its NPV."""
    return BlackCapFloorEngine.with_flat_vol(
        curve,
        SimpleQuote(ORACLE_VOL),
        DayCounter.actual365_fixed(),
        0.0,
        ORACLE_SETTINGS,
    )


def _priced(build):
    curve = _oracle_curve()
    instrument = build(_oracle_leg(curve))
    instrument.set_black_engine(_oracle_engine(curve))
    return instrument.npv()


def test_the_raw_leg_cap_reproduces_the_cached_value():
    npv = _priced(lambda leg: CapFloor.cap(leg, [CAP_RATE], ORACLE_SETTINGS))
    print(f"\nraw-leg cap npv = {npv!r} (cached {CACHED_CAP})")
    print(f"error = {abs(npv - CACHED_CAP)}")
    assert abs(npv - CACHED_CAP) <= 1.0e-11, f"{npv!r} vs cached {CACHED_CAP}"


def test_the_raw_leg_floor_reproduces_the_cached_value():
    npv = _priced(lambda leg: CapFloor.floor(leg, [FLOOR_RATE], ORACLE_SETTINGS))
    print(f"\nraw-leg floor npv = {npv!r} (cached {CACHED_FLOOR})")
    print(f"error = {abs(npv - CACHED_FLOOR)}")
    assert abs(npv - CACHED_FLOOR) <= 1.0e-11, f"{npv!r} vs cached {CACHED_FLOOR}"


def test_the_raw_leg_collar_is_the_cap_less_the_floor():
    """The tolerance is 1e-10 rather than the 1e-11 the two arms above carry:
    the collar is the difference of two values each pinned at 1e-11, so it can
    drift past that, and the core allows for it the same way at
    ``blackcapfloorengine.rs:642-664``."""
    npv = _priced(
        lambda leg: CapFloor.collar(leg, [CAP_RATE], [FLOOR_RATE], ORACLE_SETTINGS)
    )
    print(f"\nraw-leg collar npv = {npv!r} (cached {CACHED_COLLAR})")
    print(f"error = {abs(npv - CACHED_COLLAR)}")
    assert abs(npv - CACHED_COLLAR) <= 1.0e-10, f"{npv!r} vs cached {CACHED_COLLAR}"


def test_the_raw_leg_keeps_every_schedule_period():
    """What separates this path from the MakeCapFloor one above: the spot caplet
    survives, so the instrument carries one optionlet per schedule period rather
    than one fewer."""
    curve = _oracle_curve()
    leg = _oracle_leg(curve)
    cap = CapFloor.cap(leg, [CAP_RATE], ORACLE_SETTINGS)
    assert cap.coupon_count() == leg.coupon_count()
    assert cap.cap_rates() == [CAP_RATE] * cap.coupon_count()
    assert cap.floor_rates() == []


def test_a_raw_leg_constructor_needs_its_strikes():
    leg = _oracle_leg(_oracle_curve())
    with pytest.raises(ItofinError):
        CapFloor.cap(leg, [], ORACLE_SETTINGS)
    with pytest.raises(ItofinError):
        CapFloor.collar(leg, [CAP_RATE], [], ORACLE_SETTINGS)


def test_the_payment_adjustment_reaches_the_coupons():
    """The cached fixture cannot pin this setter: its schedule is already rolled
    ModifiedFollowing on TARGET, so every payment date is a business day and any
    convention leaves it alone. An UNADJUSTED schedule puts the payment dates on
    weekends, where Preceding discounts each coupon over a shorter period than
    Following does. Without this the setter could be wired to nothing and the
    three cached values above would not notice."""
    calendar = Calendar.target()
    unadjusted = BusinessDayConvention.Unadjusted
    curve = _oracle_curve()
    index = Euribor.six_months(curve, ORACLE_SETTINGS)
    schedule = Schedule(
        ORACLE_REFERENCE,
        Date(18, 3, 2012),
        Frequency.Semiannual,
        calendar,
        unadjusted,
        termination_convention=unadjusted,
    )
    leg = (
        IborLeg(schedule, index)
        .with_notional(ORACLE_NOTIONAL)
        .with_payment_day_counter(index.day_counter())
        .with_fixing_days(2)
    )

    def npv(convention):
        cap = CapFloor.cap(
            leg.with_payment_adjustment(convention), [CAP_RATE], ORACLE_SETTINGS
        )
        cap.set_black_engine(_oracle_engine(curve))
        return cap.npv()

    following = npv(BusinessDayConvention.Following)
    preceding = npv(BusinessDayConvention.Preceding)
    print(f"\nfollowing = {following!r}\npreceding = {preceding!r}")
    assert preceding > following
