"""Oracle for the Swaption + Jamshidian-engine facade (issue #501).

Reproduces the Rust cached Jamshidian European-swaption pin
(``jamshidianswaptionengine.rs:509-522``): eval 15-Jan-2026; a flat 3%
Actual365Fixed continuous/annual curve referenced 15-Jan-2026;
``HullWhite(curve, a=0.05, sigma=0.01)``; the 5Y Jamshidian-fixture swap
(nominal 100, fixed annual 15-Jan-2028 -> 15-Jan-2033 at 3% Thirty360(BondBasis)
vs semiannual Euribor6M/Actual360, zero spread); ``EuropeanExercise(15-Jan-2027)``;
Physical/PhysicalOTC.

The schedules use ``Unadjusted``, matching the cached-value fixture
(``jamshidianswaptionengine.rs:283``), so the 15-Jan-2028 / 15-Jan-2033 Saturday
endpoints stay put. This differs from the ``VanillaSwap`` oracle (#500), which
pins a separate ``ModifiedFollowing`` probe that rolls those endpoints to Monday
the 17th; the two are not the same swap fixture.

The underlying swap carries no discounting engine: the Jamshidian engine reads
the swap's arguments and prices off the Hull-White dynamics, matching the Rust
fixture (``jamshidianswaptionengine.rs:344``, which never attaches a swap engine).

    PAYER_NPV    = 1.5666103955750414   (jamshidianswaptionengine.rs:511)
    RECEIVER_NPV = 1.3562383202325612   (jamshidianswaptionengine.rs:517)
"""

import pytest

from itofin import (
    BusinessDayConvention,
    Calendar,
    Date,
    DayCounter,
    EuropeanExercise,
    Euribor,
    FlatForward,
    Frequency,
    HullWhite,
    ItofinError,
    Schedule,
    SettlementMethod,
    SettlementType,
    Settings,
    Swaption,
    SwapType,
    VanillaSwap,
)

PAYER_NPV = 1.5666103955750414
RECEIVER_NPV = 1.3562383202325612

REF = Date(15, 1, 2026)
START = Date(15, 1, 2028)
END = Date(15, 1, 2033)
EXERCISE = Date(15, 1, 2027)


def _fixture():
    settings = Settings()
    settings.set_evaluation_date(REF)
    curve = FlatForward(REF, 0.03, DayCounter.actual365_fixed())
    fixed = Schedule(
        START,
        END,
        Frequency.Annual,
        Calendar.target(),
        BusinessDayConvention.Unadjusted,
    )
    floating = Schedule(
        START,
        END,
        Frequency.Semiannual,
        Calendar.target(),
        BusinessDayConvention.Unadjusted,
    )
    index = Euribor.six_months(curve, settings)
    return settings, curve, fixed, floating, index


def _swap(swap_type, settings, curve, fixed, floating, index):
    return VanillaSwap(
        swap_type,
        100.0,
        fixed,
        0.03,
        DayCounter.thirty360_bond_basis(),
        floating,
        index,
        0.0,
        DayCounter.actual360(),
        settings,
    )


def _swaption(swap_type, settlement_type, settlement_method, attach_engine=True):
    settings, curve, fixed, floating, index = _fixture()
    swap = _swap(swap_type, settings, curve, fixed, floating, index)
    swaption = Swaption(
        swap,
        EuropeanExercise(EXERCISE),
        settlement_type,
        settlement_method,
        settings,
    )
    if attach_engine:
        model = HullWhite(curve, 0.05, 0.01)
        swaption.set_jamshidian_engine(model)
    return swaption


def test_payer_npv_matches_rust_cached_value():
    swaption = _swaption(
        SwapType.Payer, SettlementType.Physical, SettlementMethod.PhysicalOTC
    )
    assert swaption.npv() == pytest.approx(PAYER_NPV, abs=1e-8)


def test_receiver_npv_matches_rust_cached_value():
    swaption = _swaption(
        SwapType.Receiver, SettlementType.Physical, SettlementMethod.PhysicalOTC
    )
    assert swaption.npv() == pytest.approx(RECEIVER_NPV, abs=1e-8)


def test_npv_without_engine_raises_not_panics():
    swaption = _swaption(
        SwapType.Payer,
        SettlementType.Physical,
        SettlementMethod.PhysicalOTC,
        attach_engine=False,
    )
    with pytest.raises(ItofinError):
        swaption.npv()


def test_settlement_type_method_mismatch_raises_at_pricing():
    swaption = _swaption(
        SwapType.Payer, SettlementType.Cash, SettlementMethod.PhysicalOTC
    )
    with pytest.raises(ItofinError):
        swaption.npv()
