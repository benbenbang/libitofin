"""Oracle for the normal-vol Bachelier swaption engine facade (issue #616).

The twin of ``test_black_swaption.py``, and for the same reason it pins
CONSTRUCTION rather than an NPV literal: the core has no cached Bachelier
swaption value at all. Its only Bachelier engine test
(``blackswaptionengine.rs:1597``, ``swaption_delta_in_the_bachelier_model``) is
a bump-and-revalue delta self-consistency check reading a greek accessor the
Python facade does not expose, so there is no number to reproduce here.

Three pins, all on one shared ``Settings`` (the engine, the swap and the
swaption must agree on the evaluation date or the NPV is silently wrong):

A. ``BachelierSwaptionEngine(surface)`` and
   ``BachelierSwaptionEngine.with_flat_vol`` price the same swaption
   identically. ``with_flat_vol`` wraps the quote in a MOVING constant surface
   with 0 settlement days on a null calendar, so its reference date IS the
   evaluation date - which is why arm A builds its fixed-reference surface at
   exactly that date. Only then do the two routes run the identical float
   sequence, so they are pinned bit-for-bit rather than to a tolerance.
B. The same swaption on a surface referenced a month later prices differently.
   The vol level is constant, so the NPV moves only through the option time
   ``day_count(reference_date, exercise)``: a later reference shortens it,
   shrinking the variance and the premium. This is what makes arm A
   non-degenerate - it proves the engine prices rather than returning a
   constant, and that the reference date the two routes agree on is
   load-bearing.
C. A ShiftedLognormal surface handed to the Bachelier engine raises at pricing
   time, not construction (the inverse of the Black engine's refusal, and the
   direct mirror of the core test
   ``a_lognormal_volatility_surface_is_refused_by_the_bachelier_engine``,
   ``blackswaptionengine.rs:1344``). The message keys on "requires normal input
   volatility", which the Black refusal does not contain.

The volatility is quoted in absolute rate terms, not as a proportion: 50 bp of
normal vol against a 3% strike, where the lognormal twin quotes 20%.

Every arm rebuilds its own swap and swaption. An ``Instrument`` caches its NPV,
and the engine silently installs its own discounting engine on the swap it
prices, so reusing one swaption across the engines would let arm A pass on a
stale cached number.
"""

import math

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
from itofin.pricingengines import BachelierSwaptionEngine, CashAnnuityModel
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
    Schedule,
)

EVAL = Date(15, 1, 2026)
EXERCISE = Date(15, 1, 2027)
START = Date(15, 1, 2028)
END = Date(15, 1, 2033)

NORMAL_VOL = 0.0050
STRIKE = 0.03
REFERENCE_OFFSET_DAYS = 30

SETTINGS = Settings()
SETTINGS.set_evaluation_date(EVAL)


def _surface(reference_date, volatility_type=VolatilityType.Normal):
    return ConstantSwaptionVolatility(
        reference_date,
        Calendar.null_calendar(),
        BusinessDayConvention.Following,
        NORMAL_VOL,
        DayCounter.actual365_fixed(),
        volatility_type,
        0.0,
    )


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
    engine = BachelierSwaptionEngine(
        _surface(reference_date), curve, settings, CashAnnuityModel.SwapRate
    )
    swaption.set_bachelier_engine(engine)
    return swaption.npv()


def _npv_on_flat_vol():
    settings, curve, swaption = _fixture()
    engine = BachelierSwaptionEngine.with_flat_vol(
        curve,
        SimpleQuote(NORMAL_VOL),
        DayCounter.actual365_fixed(),
        0.0,
        settings,
        CashAnnuityModel.SwapRate,
    )
    swaption.set_bachelier_engine(engine)
    return swaption.npv()


def test_surface_and_flat_vol_engines_price_identically():
    npv_a = _npv_on_surface(EVAL)
    npv_b = _npv_on_flat_vol()
    print(f"\nnpv_A = {npv_a!r}\nnpv_B = {npv_b!r}")
    assert npv_a == npv_b, f"npv_A={npv_a!r} npv_B={npv_b!r}"
    assert math.isfinite(npv_a)
    assert npv_a > 0.0


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


def test_lognormal_surface_on_a_bachelier_engine_raises_at_pricing():
    settings, curve, swaption = _fixture()
    lognormal = _surface(EVAL, VolatilityType.ShiftedLognormal)
    swaption.set_bachelier_engine(
        BachelierSwaptionEngine(lognormal, curve, settings, CashAnnuityModel.SwapRate)
    )
    with pytest.raises(ItofinError) as raised:
        swaption.npv()
    assert "requires normal input volatility" in str(raised.value)
