import math

import pytest

from itofin import ItofinError
from itofin.termstructures import (
    BlackConstantVol,
    BlackVarianceCurve,
    BlackVarianceSurface,
    BlackVolTermStructure,
)
from itofin.time import Date, DayCounter

REF = Date(15, 6, 2026)


def _curve():
    """blackvariancecurve.rs:255-265: ref+180/+360/+720, vols 0.20/0.25/0.30,
    Actual360, force_monotone_variance=True."""
    dc = DayCounter.actual360()
    dates = [REF + 180, REF + 360, REF + 720]
    return BlackVarianceCurve(REF, dates, [0.20, 0.25, 0.30], dc, True)


def _surface():
    """blackvariancesurface.rs:258-282: strikes [90,100,110] x dates
    [ref+365, ref+730], Actual365Fixed, rows=strikes
    [[0.20,0.25],[0.18,0.22],[0.16,0.20]]."""
    dc = DayCounter.actual365_fixed()
    dates = [REF + 365, REF + 730]
    strikes = [90.0, 100.0, 110.0]
    matrix = [[0.20, 0.25], [0.18, 0.22], [0.16, 0.20]]
    return BlackVarianceSurface(REF, dates, strikes, matrix, dc)


def test_constant_vol_extends_base_and_is_flat():
    """blackconstantvol.rs:126,136,140: unbounded in time and strike."""
    dc = DayCounter.actual360()
    bcv = BlackConstantVol(REF, 0.30, dc)
    assert isinstance(bcv, BlackVolTermStructure)
    assert bcv.black_vol(1.0, 100.0) == pytest.approx(0.30, abs=1e-15)
    assert bcv.black_vol(5.0, 50.0) == pytest.approx(0.30, abs=1e-15)


def test_curve_extends_base_and_reproduces_nodes():
    """blackvariancecurve.rs:270-276: node vols round-trip, variances t*vol^2."""
    curve = _curve()
    assert isinstance(curve, BlackVolTermStructure)
    for date, t, vol in [(REF + 180, 0.5, 0.20), (REF + 360, 1.0, 0.25), (REF + 720, 2.0, 0.30)]:
        assert curve.black_vol_date(date, 100.0) == pytest.approx(vol, abs=1e-15)
        assert curve.black_variance(t, 100.0) == pytest.approx(t * vol * vol, abs=1e-15)
    assert curve.black_variance(1.0, 100.0) == pytest.approx(0.0625, abs=1e-15)
    assert curve.black_variance(2.0, 100.0) == pytest.approx(0.18, abs=1e-15)


def test_curve_interpolates_variance_linearly():
    """blackvariancecurve.rs:283-287: variance(0.75)=0.04125, vol=sqrt(var/t)."""
    curve = _curve()
    assert curve.black_variance(0.75, 100.0) == pytest.approx(0.04125, abs=1e-15)
    assert curve.black_vol(0.75, 100.0) == pytest.approx(math.sqrt(0.04125 / 0.75), abs=1e-15)


def test_curve_pins_zero_variance_and_forward_variance():
    """blackvariancecurve.rs:293 zero at t=0; :429-432 forward variance additive."""
    curve = _curve()
    assert curve.black_variance(0.0, 100.0) == 0.0
    assert curve.black_forward_variance(0.5, 1.0, 100.0) == pytest.approx(0.0425, abs=1e-15)


def test_curve_is_finite_in_time_and_extrapolation_is_the_escape_hatch():
    """blackvariancecurve.rs:343-347,353-356: past the last node raises with
    extrapolate=False, then flat-vol extrapolation continues past it."""
    curve = _curve()
    assert curve.black_vol(2.0, 100.0) == pytest.approx(0.30, abs=1e-15)
    with pytest.raises(ItofinError):
        curve.black_vol(2.5, 100.0, False)
    assert curve.black_variance(3.0, 100.0, True) == pytest.approx(0.27, abs=1e-15)
    assert curve.black_vol(3.0, 100.0, True) == pytest.approx(0.30, abs=1e-15)
    curve.enable_extrapolation()
    assert curve.black_vol(2.5, 100.0, False) == pytest.approx(0.30, abs=1e-15)


def test_surface_extends_base_and_reproduces_nodes():
    """blackvariancesurface.rs:301-308: node variances/vols reproduce exactly."""
    surface = _surface()
    assert isinstance(surface, BlackVolTermStructure)
    assert surface.black_variance(1.0, 100.0) == pytest.approx(0.0324, abs=1e-14)
    assert surface.black_variance(2.0, 90.0) == pytest.approx(0.125, abs=1e-14)
    assert surface.black_vol(1.0, 90.0) == pytest.approx(0.20, abs=1e-14)
    assert surface.black_vol_date(REF + 365, 100.0) == pytest.approx(0.18, abs=1e-14)


def test_surface_interpolates_bilinearly():
    """blackvariancesurface.rs:314-316: variance(0.5,100)=0.0162; the 4-corner
    mean (0.04+0.0324+0.125+0.0968)/4 = 0.07355 at (1.5, 95)."""
    surface = _surface()
    assert surface.black_variance(0.5, 100.0) == pytest.approx(0.0162, abs=1e-14)
    assert surface.black_variance(1.5, 95.0) == pytest.approx(0.07355, abs=1e-14)


def test_surface_grid_bounds_and_time_extrapolation():
    """blackvariancesurface.rs:332-336 time extrapolation past the last node;
    :368-370 grid bounds are min/max strike and the last date."""
    surface = _surface()
    assert surface.min_strike() == 90.0
    assert surface.max_strike() == 110.0
    assert surface.max_date() == REF + 730
    assert surface.black_variance(4.0, 100.0, True) == pytest.approx(0.1936, abs=1e-14)
    with pytest.raises(ItofinError):
        surface.black_variance(4.0, 100.0, False)
    surface.enable_extrapolation()
    assert surface.black_variance(4.0, 100.0, False) == pytest.approx(0.1936, abs=1e-14)


def test_surface_rejects_transposed_matrix():
    """The fixture is non-square (3 strikes x 2 dates); a transposed 2x3 matrix
    trips the core check dates.len()==columns (blackvariancesurface.rs:106-109)."""
    dc = DayCounter.actual365_fixed()
    dates = [REF + 365, REF + 730]
    strikes = [90.0, 100.0, 110.0]
    transposed = [[0.20, 0.18, 0.16], [0.25, 0.22, 0.20]]
    with pytest.raises(ItofinError):
        BlackVarianceSurface(REF, dates, strikes, transposed, dc)


def test_surface_rejects_ragged_matrix():
    """The list->Matrix converter rejects a ragged grid before construction."""
    dc = DayCounter.actual365_fixed()
    dates = [REF + 365, REF + 730]
    strikes = [90.0, 100.0, 110.0]
    ragged = [[0.20, 0.25], [0.18], [0.16, 0.20]]
    with pytest.raises(ItofinError):
        BlackVarianceSurface(REF, dates, strikes, ragged, dc)
