"""Discount jumps on the named Piecewise* curves (#1388).

QuantLib takes jumps on every PiecewiseYieldCurve constructor
(ql/termstructures/yield/piecewiseyieldcurve.hpp:74-84) and applies each jump
strictly after its date (yieldtermstructure.cpp:85). A bootstrapped curve moves
its nodes to absorb the jumps, so the oracle is interpolation-agnostic: across
the jump time the discount drops by exactly the jump factor, while the plain
curve is continuous there, and a reference-date jump never applies. Jump sizes
stay at turn-of-year magnitudes: on this short deposit strip a 0.995 jump within
the first pillar gap drives the convex-monotone forward towards zero, where its
forced-positive iterative bootstrap no longer converges.
"""

# pypi/conda library
import pytest

# itofin library
from itofin import ItofinError, Settings
from itofin.indexes import Euribor
from itofin.quotes import SimpleQuote
from itofin.termstructures import (
    DepositRateHelper,
    PiecewiseConvexMonotoneForward,
    PiecewiseCubicZero,
    PiecewiseFlatForward,
    PiecewiseLinearForward,
    PiecewiseLinearZero,
    PiecewiseLogLinearDiscount,
)
from itofin.time import Date, DayCounter
from itofin.time import Period as P

REFERENCE = Date(17, 6, 2026)
JUMP_DATES = [REFERENCE, Date(1, 8, 2026), Date(1, 3, 2027)]
JUMPS = [0.9, 0.998, 0.995]
EPS = 1e-9

CLASSES = {
    "loglinear-discount": PiecewiseLogLinearDiscount,
    "linear-zero": PiecewiseLinearZero,
    "cubic-zero": PiecewiseCubicZero,
    "linear-forward": PiecewiseLinearForward,
    "convex-monotone-iterative": PiecewiseConvexMonotoneForward,
    "convex-monotone-local": lambda *a, **kw: PiecewiseConvexMonotoneForward(*a, bootstrap="local", **kw),
    "flat-forward": PiecewiseFlatForward,
}


def _curve(kind, **kwargs):
    settings = Settings()
    settings.set_evaluation_date(Date(15, 6, 2026))
    helpers = [
        DepositRateHelper(SimpleQuote(0.03 + 0.002 * i), Euribor(P(months, "Months"), None, settings))
        for i, months in enumerate([1, 3, 6, 12])
    ]
    return CLASSES[kind](REFERENCE, helpers, DayCounter.actual365_fixed(), **kwargs)


def _drop(curve, t):
    return curve.discount(t + EPS, True) / curve.discount(t, True)


@pytest.mark.parametrize("kind", CLASSES)
def test_discount_drops_by_the_jump_across_its_date(kind):
    plain = _curve(kind)
    quotes = [SimpleQuote(v) for v in JUMPS]
    jumped = _curve(kind, jumps=quotes, jump_dates=JUMP_DATES)
    assert jumped.jump_dates() == JUMP_DATES
    times = jumped.jump_times()
    assert times[0] == 0.0
    assert jumped.discount(EPS, True) == pytest.approx(1.0, abs=1e-8)
    for t, v in zip(times[1:], JUMPS[1:]):
        assert _drop(jumped, t) == pytest.approx(v, abs=1e-8)
        assert _drop(plain, t) == pytest.approx(1.0, abs=1e-8)
    quotes[2].set_value(0.993)
    assert _drop(jumped, times[2]) == pytest.approx(0.993, abs=1e-8)
    assert _drop(jumped, times[1]) == pytest.approx(0.998, abs=1e-8)


def test_loglinear_discount_nodes_absorb_the_jumps():
    plain = _curve("loglinear-discount")
    jumped = _curve("loglinear-discount", jumps=[SimpleQuote(v) for v in JUMPS], jump_dates=JUMP_DATES)
    assert jumped.dates() == plain.dates()
    key = [(d.year, d.month, d.day) for d in jumped.dates()]
    for i, (k, j, p) in enumerate(zip(key, jumped.data(), plain.data())):
        product = 1.0
        for d, v in zip(JUMP_DATES[1:], JUMPS[1:]):
            if (d.year, d.month, d.day) < k:
                product *= v
        assert j * product == pytest.approx(p, abs=1e-12), i


@pytest.mark.parametrize("kind", CLASSES)
def test_empty_jump_dates_generate_year_ends(kind):
    curve = _curve(kind, jumps=[SimpleQuote(0.995), SimpleQuote(0.99)])
    assert curve.jump_dates() == [Date(31, 12, 2026), Date(31, 12, 2027)]


@pytest.mark.parametrize("kind", CLASSES)
def test_no_jumps_is_the_shipped_curve(kind):
    plain, default = _curve(kind), _curve(kind, jumps=None, jump_dates=None)
    assert default.jump_dates() == []
    assert default.data() == plain.data()


@pytest.mark.parametrize("kind", CLASSES)
def test_jump_argument_mismatch_raises(kind):
    with pytest.raises(ItofinError, match="mismatch"):
        _curve(kind, jumps=[SimpleQuote(0.99)], jump_dates=JUMP_DATES)
