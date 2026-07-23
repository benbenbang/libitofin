"""Oracle for the VanillaSwap facade (issue #500).

The Jamshidian-fixture 5Y swap: nominal 100, fixed leg annual
15-Jan-2028 -> 15-Jan-2033 at 3% on Thirty360(BondBasis), floating leg
semiannual Euribor6M on Actual360 with zero spread, Payer, priced with a
``DiscountingSwapEngine`` on a flat 3% curve (Actual365Fixed, continuous,
referenced 15-Jan-2026).

The expected ``fair_rate`` and ``npv`` were pinned against an independently
constructed Rust probe (a throwaway ``cargo test`` that built the identical
fixture through ``VanillaSwap::new(...).into_fixed_vs_floating()`` with a
``DiscountingSwapEngine`` and printed the results to 17 significant digits;
reverted, not committed):

    PROBE_FAIR      = 0.03048844643136293
    PROBE_NPV       = 0.21033895380698553   (Payer)
    PROBE_RECV_NPV  = -0.21033895380698553  (Receiver)

The schedule endpoints 15-Jan-2028 and 15-Jan-2033 are both Saturdays and roll
to Monday 17-Jan under TARGET + ModifiedFollowing; the facade Schedule
reproduces the core exactly, so the fixture stays apples-to-apples.
"""

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import Euribor
from itofin.instruments import SwapType, VanillaSwap
from itofin.termstructures import FlatForward
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    DayCounter,
    Frequency,
    Schedule,
)

PROBE_FAIR = 0.03048844643136293
PROBE_NPV = 0.21033895380698553

REF = Date(15, 1, 2026)
START = Date(15, 1, 2028)
END = Date(15, 1, 2033)


def _fixture():
    settings = Settings()
    settings.set_evaluation_date(REF)
    curve = FlatForward(REF, 0.03, DayCounter.actual365_fixed())
    fixed = Schedule(
        START,
        END,
        Frequency.Annual,
        Calendar.target(),
        BusinessDayConvention.ModifiedFollowing,
    )
    floating = Schedule(
        START,
        END,
        Frequency.Semiannual,
        Calendar.target(),
        BusinessDayConvention.ModifiedFollowing,
    )
    index = Euribor.six_months(curve, settings)
    return settings, curve, fixed, floating, index


def _swap(swap_type):
    settings, curve, fixed, floating, index = _fixture()
    swap = VanillaSwap(
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
    swap.set_engine(curve, settings)
    return swap


def test_fair_rate_matches_rust_probe():
    swap = _swap(SwapType.Payer)
    assert swap.fair_rate() == pytest.approx(PROBE_FAIR, abs=1e-10)


def test_npv_matches_rust_probe():
    swap = _swap(SwapType.Payer)
    assert swap.npv() == pytest.approx(PROBE_NPV, abs=1e-10)


def test_payer_receiver_npv_are_opposite():
    payer = _swap(SwapType.Payer)
    receiver = _swap(SwapType.Receiver)
    assert receiver.npv() == pytest.approx(-payer.npv(), abs=1e-10)


def test_nominal_and_fixed_rate_round_trip():
    swap = _swap(SwapType.Payer)
    assert swap.nominal() == pytest.approx(100.0, abs=1e-12)
    assert swap.fixed_rate() == pytest.approx(0.03, abs=1e-12)


def test_price_without_engine_raises_not_panics():
    settings, curve, fixed, floating, index = _fixture()
    swap = VanillaSwap(
        SwapType.Payer,
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
    with pytest.raises(ItofinError):
        swap.fair_rate()
