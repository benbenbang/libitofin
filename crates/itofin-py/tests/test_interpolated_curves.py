import math

import pytest

from itofin import ItofinError
from itofin.termstructures import (
    DiscountCurve,
    ForwardCurve,
    YieldTermStructure,
    ZeroCurve,
)
from itofin.time import Date, DayCounter

REF = Date(15, 6, 2026)


def _dates():
    """zerocurve.rs:201-208 / discountcurve.rs:189-192: ref, +180, +360, +720."""
    return [REF, REF + 180, REF + 360, REF + 720]


def _zero_curve():
    """zerocurve.rs:210-216: zeros 0.02/0.03/0.04/0.045, Actual360, Linear."""
    return ZeroCurve(_dates(), [0.02, 0.03, 0.04, 0.045], DayCounter.actual360())


def _discount_curve():
    """discountcurve.rs:194-200: discounts 1.0/0.97/0.94/0.88, Actual360, LogLinear."""
    return DiscountCurve(_dates(), [1.0, 0.97, 0.94, 0.88], DayCounter.actual360())


def _forward_curve():
    """forwardcurve.rs:328-336: dates ref/+360/+720, forwards 0.03/0.04/0.06,
    Actual360, BackwardFlat."""
    return ForwardCurve([REF, REF + 360, REF + 720], [0.03, 0.04, 0.06], DayCounter.actual360())


def test_discount_curve_extends_base_and_reproduces_nodes():
    """discountcurve.rs:203-211: reference date is the first node, max date the
    last, node discounts round-trip through discount_date @1e-15."""
    curve = _discount_curve()
    assert isinstance(curve, YieldTermStructure)
    assert curve.reference_date() == REF
    assert curve.max_date() == REF + 720
    for date, discount in zip(_dates(), [1.0, 0.97, 0.94, 0.88]):
        assert curve.discount_date(date) == pytest.approx(discount, abs=1e-15)


def test_discount_curve_log_linear_midpoint():
    """discountcurve.rs:217-218: log-linear interpolation is geometric,
    discount(0.75) == sqrt(0.97 * 0.94)."""
    curve = _discount_curve()
    assert curve.discount(0.75) == pytest.approx(math.sqrt(0.97 * 0.94), abs=1e-15)


def test_discount_curve_zero_rate_round_trips_the_discount():
    """discountcurve.rs:256-258: continuously-compounded zero_rate(1.0) == -ln(0.94)."""
    curve = _discount_curve()
    assert curve.zero_rate(1.0) == pytest.approx(-math.log(0.94), abs=1e-14)


def test_discount_curve_extrapolation_is_the_escape_hatch():
    """discountcurve.rs:234-245: past the last node discount raises with the
    default extrapolate=False; both extrapolate=True and enable_extrapolation()
    continue the last forward flat, expected 0.88 * exp(-ln(0.94/0.88))."""
    curve = _discount_curve()
    expected = 0.88 * math.exp(-math.log(0.94 / 0.88))
    with pytest.raises(ItofinError):
        curve.discount(3.0, False)
    assert curve.discount(3.0, True) == pytest.approx(expected, abs=1e-14)
    curve.enable_extrapolation()
    assert curve.discount(3.0, False) == pytest.approx(expected, abs=1e-14)


def test_discount_curve_constructor_rejects_bad_inputs():
    """discountcurve.rs:287-333: length mismatch, first discount != 1.0, and
    unsorted dates each raise ItofinError instead of panicking."""
    dc = DayCounter.actual360()
    with pytest.raises(ItofinError):
        DiscountCurve(_dates(), [1.0, 0.97], dc)
    with pytest.raises(ItofinError):
        DiscountCurve(_dates(), [0.99, 0.97, 0.94, 0.88], dc)
    unsorted = [REF, REF + 360, REF + 180, REF + 720]
    with pytest.raises(ItofinError):
        DiscountCurve(unsorted, [1.0, 0.97, 0.94, 0.88], dc)


def test_zero_curve_extends_base_and_reproduces_the_zero_rates():
    """zerocurve.rs:218-229: zero_rate(t) reads back the node rate and
    discount(t) == exp(-z*t); discount(0.0) is exactly 1.0."""
    curve = _zero_curve()
    assert isinstance(curve, YieldTermStructure)
    for t, z in [(0.5, 0.03), (1.0, 0.04), (2.0, 0.045)]:
        assert curve.zero_rate(t) == pytest.approx(z, abs=1e-14)
        assert curve.discount(t) == pytest.approx(math.exp(-z * t), abs=1e-15)
    assert curve.discount(0.0) == 1.0


def test_zero_curve_interpolates_rates_linearly():
    """zerocurve.rs:238-245: zero_rate(0.75) == 0.035 (midpoint of 0.03/0.04)
    and discount(1.5) == exp(-0.0425 * 1.5)."""
    curve = _zero_curve()
    assert curve.zero_rate(0.75) == pytest.approx(0.035, abs=1e-14)
    assert curve.discount(1.5) == pytest.approx(math.exp(-0.0425 * 1.5), abs=1e-15)


def test_zero_curve_constructor_rejects_bad_inputs():
    """zerocurve.rs:319-331: length mismatch and unsorted dates raise ItofinError."""
    dc = DayCounter.actual360()
    with pytest.raises(ItofinError):
        ZeroCurve(_dates(), [0.02], dc)
    unsorted = [REF, REF + 360, REF + 180, REF + 720]
    with pytest.raises(ItofinError):
        ZeroCurve(unsorted, [0.02, 0.03, 0.04, 0.045], dc)


def test_forward_curve_extends_base_and_discounts_backward_flat():
    """forwardcurve.rs:347-348: backward-flat forwards 0.04 on (0,1] and 0.06
    on (1,2] integrate to discount(2.0) == exp(-0.10)."""
    curve = _forward_curve()
    assert isinstance(curve, YieldTermStructure)
    assert curve.reference_date() == REF
    assert curve.max_date() == REF + 720
    assert curve.discount(2.0) == pytest.approx(math.exp(-0.10), abs=1e-15)


def test_forward_curve_extrapolation_is_the_escape_hatch():
    """forwardcurve.rs:358-360: discount past the last node raises with the
    default extrapolate=False, then enable_extrapolation() answers it."""
    curve = _forward_curve()
    with pytest.raises(ItofinError):
        curve.discount(3.0, False)
    curve.enable_extrapolation()
    assert curve.discount(3.0, False) > 0.0


def test_forward_curve_constructor_rejects_bad_inputs():
    """forwardcurve.rs:425-455: length mismatch and unsorted dates raise ItofinError."""
    dc = DayCounter.actual360()
    with pytest.raises(ItofinError):
        ForwardCurve([REF, REF + 360], [0.03], dc)
    with pytest.raises(ItofinError):
        ForwardCurve([REF + 360, REF], [0.03, 0.04], dc)


# --- Cubic on standalone curves (#547) ---------------------------------------
#
# Mirrors the core smoke test zerocurve.rs:334-354
# (cubic_factory_backs_a_standalone_zero_curve): the Cubic (Kruger) interpolant
# passes through its nodes and evaluates finite between them. Cubic is
# non-monotonic (no Hyman filter, cubic.rs:712-713), so we assert node
# reproduction + finiteness ONLY - never "between neighbours".


def test_zero_curve_accepts_cubic_and_reproduces_nodes():
    """PyZeroCurve gains interpolation="Cubic". The zero rates round-trip at the
    node times to 1e-12 and an interior point is finite (not bounded)."""
    curve = ZeroCurve(_dates(), [0.02, 0.03, 0.04, 0.045], DayCounter.actual360(), "Cubic")
    for t, zero in [(0.5, 0.03), (1.0, 0.04), (2.0, 0.045)]:
        assert curve.zero_rate(t) == pytest.approx(zero, abs=1e-12)
    assert math.isfinite(curve.zero_rate(0.75))


def test_discount_curve_accepts_cubic_and_reproduces_nodes():
    """PyDiscountCurve gains interpolation="Cubic" (trailing after calendar). The
    node discounts round-trip to 1e-12 and an interior point is finite."""
    curve = DiscountCurve(
        _dates(), [1.0, 0.97, 0.94, 0.88], DayCounter.actual360(), None, "Cubic"
    )
    for t, discount in [(0.5, 0.97), (1.0, 0.94), (2.0, 0.88)]:
        assert curve.discount(t) == pytest.approx(discount, abs=1e-12)
    assert math.isfinite(curve.discount(1.5))


def test_standalone_curves_reject_unknown_interpolation():
    """The new string arms name their valid interpolators: ZeroCurve is
    Linear|Cubic, DiscountCurve is LogLinear|Cubic."""
    dc = DayCounter.actual360()
    with pytest.raises(ItofinError):
        ZeroCurve(_dates(), [0.02, 0.03, 0.04, 0.045], dc, "LogLinear")
    with pytest.raises(ItofinError):
        DiscountCurve(_dates(), [1.0, 0.97, 0.94, 0.88], dc, None, "Linear")
