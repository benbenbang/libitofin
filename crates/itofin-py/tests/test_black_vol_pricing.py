import pytest

from itofin import Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.processes import BlackScholesProcess
from itofin.termstructures import BlackConstantVol, BlackVarianceSurface, FlatForward
from itofin.time import Date, DayCounter

REF = Date(15, 6, 2026)


def _surface():
    """blackvariancesurface.rs:258-282: strikes [90,100,110] x dates
    [ref+365, ref+730], Actual365Fixed, rows=strikes
    [[0.20,0.25],[0.18,0.22],[0.16,0.20]]. Node vol at (strike 100, ref+365)
    is 0.18 (blackvariancesurface.rs:305)."""
    dc = DayCounter.actual365_fixed()
    dates = [REF + 365, REF + 730]
    strikes = [90.0, 100.0, 110.0]
    matrix = [[0.20, 0.25], [0.18, 0.22], [0.16, 0.20]]
    return BlackVarianceSurface(REF, dates, strikes, matrix, dc)


def _gate_row_1_npv(proc):
    s = Settings()
    s.set_evaluation_date(REF)
    opt = VanillaOption(OptionType.Call, 65.0, REF + 90, s)
    opt.set_engine(proc)
    return opt.npv()


def test_from_curves_reproduces_scalar_gate_row_1_npv_bit_for_bit():
    """The object ctor (two FlatForwards + BlackConstantVol) must reproduce the
    scalar ctor's GATE row-1 process exactly (test_european_option.py:15,23),
    pinning the dividend-before-risk-free order on the new constructor. Both are
    priced live and compared bit-for-bit: the scalar literal 2.1333684449161985
    is itself 1 ULP off the current build, so the scalar path is the oracle."""
    dc = DayCounter.actual360()
    scalar = BlackScholesProcess(60.0, 0.08, 0.0, 0.30, REF, dc)
    obj = BlackScholesProcess.from_curves(
        60.0,
        FlatForward(REF, 0.08, dc),
        FlatForward(REF, 0.0, dc),
        BlackConstantVol(REF, 0.30, dc),
    )
    assert _gate_row_1_npv(obj) == _gate_row_1_npv(scalar)
    assert _gate_row_1_npv(obj) == pytest.approx(2.1333684449161985, abs=1e-10)


def test_european_prices_off_surface_equal_constant_vol_at_a_grid_node():
    """A European struck/expiring exactly on the surface grid (strike 100,
    expiry ref+365, node vol 0.18 at blackvariancesurface.rs:305) prices
    identically to a BlackConstantVol(0.18) process sharing the same r/q/spot;
    bilinear interpolation is exact at a node, so the NPVs coincide."""
    s = Settings()
    s.set_evaluation_date(REF)
    dc = DayCounter.actual365_fixed()
    risk_free = FlatForward(REF, 0.05, dc)
    dividend = FlatForward(REF, 0.0, dc)

    surface = _surface()
    surface_proc = BlackScholesProcess.from_curves(100.0, risk_free, dividend, surface)
    surface_opt = VanillaOption(OptionType.Call, 100.0, REF + 365, s)
    surface_opt.set_engine(surface_proc)

    const_vol = BlackConstantVol(REF, 0.18, dc)
    const_proc = BlackScholesProcess.from_curves(100.0, risk_free, dividend, const_vol)
    const_opt = VanillaOption(OptionType.Call, 100.0, REF + 365, s)
    const_opt.set_engine(const_proc)

    surface_npv = surface_opt.npv()
    assert surface_npv > 0.0
    assert surface_npv == pytest.approx(const_opt.npv(), abs=1e-12)
