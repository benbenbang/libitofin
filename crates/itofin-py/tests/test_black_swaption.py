"""Oracle for the swaption vol-structure + Black-engine facade (issue #612).

The bindings oracle is the wrapped Rust numbers, and this pass proves the
CONSTRUCTION is right rather than reproducing a swap NPV literal: the core's
``blackswaptionengine.rs`` fixture is built with ``MakeVanillaSwap``, which has
no Python facade yet, so its literal belongs to a later ticket.

Three pins, all on one shared ``Settings`` (the engine, the swap and the
swaption must agree on the evaluation date or the NPV is silently wrong):

A. ``ConstantSwaptionVolatility`` returns the constructed vol for any query.
B. ``BlackSwaptionEngine(surface)`` and ``BlackSwaptionEngine.with_flat_vol``
   price the same swaption identically. ``with_flat_vol`` wraps the quote in a
   moving constant surface with 0 settlement days on a null calendar, so its
   reference date IS the evaluation date - the same surface arm A builds with a
   fixed reference at that date. The two routes run the identical float
   sequence, so they are pinned bit-for-bit, not to a tolerance.
C. The same swaption on a surface referenced a month later prices differently.
   The vol level is constant, so the NPV moves only through the option time
   ``day_count(reference_date, exercise)``: a later reference shortens it,
   shrinking the variance and the premium. This is what makes B non-degenerate -
   it proves the reference date the two routes agree on is load-bearing.

Every arm rebuilds its own swap and swaption. An ``Instrument`` caches its NPV,
and the engine silently installs its own discounting engine on the swap it
prices, so reusing one swaption across the three engines would let arm B pass on
a stale cached number.
"""

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import Euribor
from itofin.instruments import (
    EuropeanExercise,
    SettlementMethod,
    SettlementType,
    Swaption,
    SwapType,
    VanillaSwap,
)
from itofin.pricingengines import BlackSwaptionEngine, CashAnnuityModel
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    ConstantSwaptionVolatility,
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
EXERCISE = Date(15, 1, 2027)
START = Date(15, 1, 2028)
END = Date(15, 1, 2033)

VOL = 0.20
STRIKE = 0.03
REFERENCE_OFFSET_DAYS = 30


def _surface(reference_date):
    return ConstantSwaptionVolatility(
        reference_date,
        Calendar.null_calendar(),
        BusinessDayConvention.Following,
        VOL,
        DayCounter.actual365_fixed(),
        VolatilityType.ShiftedLognormal,
        0.0,
    )


SETTINGS = Settings()
SETTINGS.set_evaluation_date(EVAL)


def _fixture():
    """A fresh curve/swap/swaption on the one shared ``Settings``."""
    settings = SETTINGS
    curve = FlatForward(EVAL, 0.03, DayCounter.actual365_fixed())
    fixed = Schedule(
        START, END, Frequency.Annual, Calendar.target(), BusinessDayConvention.Unadjusted
    )
    floating = Schedule(
        START,
        END,
        Frequency.Semiannual,
        Calendar.target(),
        BusinessDayConvention.Unadjusted,
    )
    swap = VanillaSwap(
        SwapType.Payer,
        100.0,
        fixed,
        STRIKE,
        DayCounter.thirty360_bond_basis(),
        floating,
        Euribor.six_months(curve, settings),
        0.0,
        DayCounter.actual360(),
        settings,
    )
    swaption = Swaption(
        swap,
        EuropeanExercise(EXERCISE),
        SettlementType.Physical,
        SettlementMethod.PhysicalOTC,
        settings,
    )
    return settings, curve, swaption


def _npv_on_surface(reference_date):
    settings, curve, swaption = _fixture()
    engine = BlackSwaptionEngine(
        _surface(reference_date), curve, settings, CashAnnuityModel.SwapRate
    )
    swaption.set_black_engine(engine)
    return swaption.npv()


def _npv_on_flat_vol():
    settings, curve, swaption = _fixture()
    engine = BlackSwaptionEngine.with_flat_vol(
        curve,
        SimpleQuote(VOL),
        DayCounter.actual365_fixed(),
        0.0,
        settings,
        CashAnnuityModel.SwapRate,
    )
    swaption.set_black_engine(engine)
    return swaption.npv()


def test_constant_surface_returns_the_constructed_volatility():
    surface = _surface(EVAL)
    for option_tenor, swap_tenor in [(1, 5), (2, 10), (10, 30)]:
        assert (
            surface.volatility(
                Period(option_tenor, "Years"), Period(swap_tenor, "Years"), STRIKE, False
            )
            == VOL
        )


def test_quote_backed_surface_tracks_its_quote():
    quote = SimpleQuote(VOL)
    surface = ConstantSwaptionVolatility.with_quote(
        EVAL,
        Calendar.null_calendar(),
        BusinessDayConvention.Following,
        quote,
        DayCounter.actual365_fixed(),
        VolatilityType.ShiftedLognormal,
        0.0,
    )
    one_by_five = (Period(1, "Years"), Period(5, "Years"), STRIKE, False)
    assert surface.volatility(*one_by_five) == VOL
    quote.set_value(0.25)
    assert surface.volatility(*one_by_five) == 0.25


def test_surface_and_flat_vol_engines_price_identically():
    npv_a = _npv_on_surface(EVAL)
    npv_b = _npv_on_flat_vol()
    assert npv_a == npv_b, f"npv_A={npv_a!r} npv_B={npv_b!r}"


def test_a_later_reference_date_moves_the_npv():
    npv_a = _npv_on_surface(EVAL)
    npv_c = _npv_on_surface(EVAL + REFERENCE_OFFSET_DAYS)
    print(
        f"\nnpv_A (reference {EVAL!r}) = {npv_a!r}"
        f"\nnpv_C (reference {EVAL + REFERENCE_OFFSET_DAYS!r}) = {npv_c!r}"
        f"\ngap = {npv_a - npv_c!r}"
    )
    assert npv_c != npv_a
    assert npv_c < npv_a


def test_normal_surface_on_a_black_engine_raises_at_pricing():
    settings, curve, swaption = _fixture()
    normal = ConstantSwaptionVolatility(
        EVAL,
        Calendar.null_calendar(),
        BusinessDayConvention.Following,
        VOL,
        DayCounter.actual365_fixed(),
        VolatilityType.Normal,
        0.0,
    )
    swaption.set_black_engine(
        BlackSwaptionEngine(normal, curve, settings, CashAnnuityModel.SwapRate)
    )
    with pytest.raises(ItofinError):
        swaption.npv()
