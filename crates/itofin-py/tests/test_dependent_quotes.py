# stdlib
import math
from typing import Any, cast

# pypi/conda library
import pytest

# itofin library
from itofin.quotes import CompositeOp, CompositeQuote, DerivedQuote, Quote, SimpleQuote
from itofin.termstructures import FlatForward
from itofin.time import Date, DayCounter


@pytest.mark.parametrize(
    ("a", "b", "f"),
    [(1.0, 10.0, lambda x: x + 10.0), (10.0, 0.0, lambda x: x * 10.0), (1.0, -10.0, lambda x: x - 10.0)],
)
def test_derived_quote_matches_quantlib_test_derived(a, b, f):
    me = SimpleQuote(0.0)
    derived = DerivedQuote(me, a, b)
    for value in (12.0, 23.0, 34.0):
        me.set_value(value)
        assert derived.value() == pytest.approx(f(value), abs=1e-10)


@pytest.mark.parametrize(
    ("op", "f"),
    [
        (CompositeOp.Add, lambda x, y: x + y),
        (CompositeOp.Multiply, lambda x, y: x * y),
        (CompositeOp.Subtract, lambda x, y: x - y),
    ],
)
def test_composite_quote_matches_quantlib_test_composite(op, f):
    me1, me2 = SimpleQuote(0.0), SimpleQuote(0.0)
    composite = CompositeQuote(me1, me2, op)
    for value in (12.0, 23.0, 34.0):
        me1.set_value(value)
        me2.set_value(value + 1.0)
        assert composite.value() == pytest.approx(f(value, value + 1.0), abs=1e-10)


def test_chained_dependents_track_the_source():
    x = SimpleQuote(0.75)
    square = CompositeQuote(x, x, CompositeOp.Multiply)
    chained = DerivedQuote(DerivedQuote(x, a=3.0), 2.0, 1.0)
    mixed = CompositeQuote(square, chained, CompositeOp.Add)
    assert mixed.value() == 6.0625
    x.set_value(2.0)
    assert (square.value(), chained.value(), mixed.value()) == (4.0, 13.0, 17.0)
    assert all(isinstance(q, Quote) for q in (x, square, chained, mixed))


def test_flat_forward_on_a_dependent_quote_reprices():
    base = SimpleQuote(0.01)
    curve = FlatForward.from_quote(Date(15, 6, 2026), DerivedQuote(base, 2.0), DayCounter.actual360())
    assert curve.discount(2.0) == pytest.approx(math.exp(-0.04), abs=1e-15)
    base.set_value(0.02)
    assert curve.discount(2.0) == pytest.approx(math.exp(-0.08), abs=1e-15)


def test_invalid_inputs_raise():
    q = SimpleQuote(1.0)
    with pytest.raises(TypeError):
        CompositeQuote(q, q, cast(Any, 3))
    with pytest.raises(TypeError):
        DerivedQuote(cast(Any, Date(15, 6, 2026)))
    with pytest.raises(TypeError):
        Quote()
    assert not hasattr(DerivedQuote(q), "set_value")
