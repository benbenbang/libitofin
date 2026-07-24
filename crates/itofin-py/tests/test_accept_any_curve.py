"""Oracle for issue #527: the four pricing facades accept ANY yield curve.

The four sites (HullWhite ctor, Euribor.six_months, SwaptionHelper ctor,
VanillaSwap.set_engine) were widened from ``&PyFlatForward`` to the
``&PyYieldTermStructure`` base, so a real ``ZeroCurve`` / ``DiscountCurve`` now
flows through where only a flat curve used to. The discriminating oracle is
degenerate equivalence: a constant-zero ``ZeroCurve`` as the risk-free leg plus
an all-``sigma`` ``BlackVarianceSurface`` as the vol leg must reproduce the
flat-scalar constructor's European NPV to 1e-12; a mis-wired handle (or a
swapped r/q) would not.
"""

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import Euribor
from itofin.instruments import (
    OptionType,
    SwapType,
    VanillaOption,
    VanillaSwap,
)
from itofin.models import HullWhite
from itofin.processes import BlackScholesProcess
from itofin.termstructures import (
    BlackVarianceSurface,
    FlatForward,
    YieldTermStructure,
    ZeroCurve,
)
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    DayCounter,
    Frequency,
    Schedule,
)

REF = Date(15, 6, 2026)
R = 0.05
Q = 0.02
SIGMA = 0.20
SPOT = 100.0
STRIKE = 100.0


def _degenerate_zero_curve():
    """A ZeroCurve whose nodes are all the flat rate R, spanning the REF+365
    option node (last node REF+730). Constant zeros make discount(t) == exp(-R*t),
    identical to FlatForward(R, Continuous)."""
    dc = DayCounter.actual360()
    dates = [REF, REF + 365, REF + 730]
    return ZeroCurve(dates, [R, R, R], dc)


def _degenerate_surface():
    """A BlackVarianceSurface whose every grid vol equals SIGMA, spanning the
    REF+365 option node (dates REF+90/REF+365/REF+730, strikes 90/100/110).
    A constant surface has variance == SIGMA**2 * t, identical to
    BlackConstantVol(SIGMA)."""
    dc = DayCounter.actual360()
    dates = [REF + 90, REF + 365, REF + 730]
    strikes = [90.0, 100.0, 110.0]
    matrix = [[SIGMA, SIGMA, SIGMA] for _ in strikes]
    return BlackVarianceSurface(REF, dates, strikes, matrix, dc)


def _npv(process, expiry):
    s = Settings()
    s.set_evaluation_date(REF)
    opt = VanillaOption(OptionType.Call, STRIKE, expiry, s)
    opt.set_engine(process)
    return opt.npv()


def test_zero_curve_and_surface_reproduce_the_flat_scalar_npv():
    """The discriminating oracle. A European struck on a shared grid node
    (strike 100, expiry REF+365), priced off a constant-zero ZeroCurve
    risk-free leg and an all-SIGMA BlackVarianceSurface vol leg, reproduces the
    flat-scalar BlackScholesProcess(SPOT, R, Q, SIGMA) NPV to 1e-12. r != q, so
    an r/q swap on the object path would break the equality while both remain
    finite."""
    dc = DayCounter.actual360()
    scalar = BlackScholesProcess(SPOT, R, Q, SIGMA, REF, dc)
    obj = BlackScholesProcess.from_curves(
        SPOT,
        _degenerate_zero_curve(),
        FlatForward(REF, Q, dc),
        _degenerate_surface(),
    )
    node = REF + 365
    obj_npv = _npv(obj, node)
    assert obj_npv > 0.0
    assert obj_npv == pytest.approx(_npv(scalar, node), abs=1e-12)


def test_grid_not_spanning_the_expiry_raises():
    """A European expiring past the curve/surface max date (REF+900 > REF+730)
    raises ItofinError under the default extrapolate=False rather than returning
    a fabricated number: the ZeroCurve is the first structure the engine queries
    out of range."""
    obj = BlackScholesProcess.from_curves(
        SPOT,
        _degenerate_zero_curve(),
        FlatForward(REF, Q, DayCounter.actual360()),
        _degenerate_surface(),
    )
    with pytest.raises(ItofinError):
        _npv(obj, REF + 900)


def test_hull_white_on_zero_curve_reproduces_flat_r0():
    """HullWhite reads its initial short rate r0 as the fitted forward at 0. A
    constant-zero ZeroCurve at 0.03 has forward(0) == 0.03, so it reproduces the
    flat FlatForward(0.03) model's r0 exactly. A mis-wired handle would not."""
    dc = DayCounter.actual360()
    flat_hw = HullWhite(FlatForward(REF, 0.03, dc), 0.05, 0.01)
    zero_hw = HullWhite(
        ZeroCurve([REF, REF + 365, REF + 730], [0.03, 0.03, 0.03], dc), 0.05, 0.01
    )
    assert zero_hw.r0() == pytest.approx(0.03, abs=1e-12)
    assert zero_hw.r0() == pytest.approx(flat_hw.r0(), abs=1e-12)


def test_euribor_six_months_accepts_a_zero_curve():
    """The widened Euribor.six_months constructs against a ZeroCurve, not only a
    FlatForward."""
    settings = Settings()
    settings.set_evaluation_date(REF)
    dc = DayCounter.actual360()
    curve = ZeroCurve([REF, REF + 365, REF + 730], [0.03, 0.03, 0.03], dc)
    index = Euribor.six_months(curve, settings)
    assert index is not None


def test_vanilla_swap_set_engine_prices_off_a_zero_curve():
    """VanillaSwap.set_engine accepts a real ZeroCurve. A constant-zero curve at
    0.03 (Actual365Fixed, continuous) discounts identically to the flat 3% curve
    the Jamshidian fixture pins, so the Payer NPV reproduces the flat probe
    0.21033895380698553 (test_vanilla_swap.py)."""
    swap_ref = Date(15, 1, 2026)
    start = Date(15, 1, 2028)
    end = Date(15, 1, 2033)
    settings = Settings()
    settings.set_evaluation_date(swap_ref)
    a365 = DayCounter.actual365_fixed()
    index_curve = FlatForward(swap_ref, 0.03, a365)
    fixed = Schedule(
        start,
        end,
        Frequency.Annual,
        Calendar.target(),
        BusinessDayConvention.ModifiedFollowing,
    )
    floating = Schedule(
        start,
        end,
        Frequency.Semiannual,
        Calendar.target(),
        BusinessDayConvention.ModifiedFollowing,
    )
    index = Euribor.six_months(index_curve, settings)
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
    discount_curve = ZeroCurve([swap_ref, Date(15, 1, 2035)], [0.03, 0.03], a365)
    swap.set_engine(discount_curve, settings)
    assert swap.npv() == pytest.approx(0.21033895380698553, abs=1e-10)


def test_zero_curve_is_a_yield_term_structure():
    """The T0 hierarchy holds: a ZeroCurve is a YieldTermStructure instance, the
    property the widened base-typed parameters rely on."""
    dc = DayCounter.actual360()
    curve = ZeroCurve([REF, REF + 365, REF + 730], [R, R, R], dc)
    assert isinstance(curve, YieldTermStructure)
