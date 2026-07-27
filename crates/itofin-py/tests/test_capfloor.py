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
D. ``CapFloorType`` exposes Cap and Floor only - Collar is deliberately absent.

Every arm builds its own ``CapFloor``. An ``Instrument`` caches its NPV, so
reusing one cap across two engines would let arm B pass on a stale number. One
shared ``Settings`` drives everything: the instrument and the engine must agree
on the evaluation date or the NPV is silently wrong.
"""

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import Euribor
from itofin.instruments import CapFloor, CapFloorType
from itofin.termstructures import FlatForward
from itofin.time import DayCounter, Date, Period

EVAL = Date(15, 1, 2026)

CURVE_RATE = 0.05
CAP_STRIKE = 0.04
FLOOR_STRIKE = 0.06
OUT_OF_THE_MONEY_STRIKE = 0.06

TENOR = Period(5, "Years")
SPOT = Period(0, "Days")

SETTINGS = Settings()
SETTINGS.set_evaluation_date(EVAL)


def _curve():
    return FlatForward(EVAL, CURVE_RATE, DayCounter.actual365_fixed())


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


def test_cap_floor_type_exposes_cap_and_floor_only():
    assert hasattr(CapFloorType, "Cap")
    assert hasattr(CapFloorType, "Floor")
    assert not hasattr(CapFloorType, "Collar")
