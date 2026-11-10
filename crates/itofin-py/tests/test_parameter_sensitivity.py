"""Parameter sensitivities of dependent quotes to a SimpleQuote bump.

The oracle is the analytic core test crates/libitofin/tests/parameter_sensitivity.rs:
there is no QuantLib test-suite case, the parameter forms were removed upstream.
The values are exact binary fractions, so the asserts are exact.
"""

# standard library
import math
from typing import Any, cast

# third party
import pytest

# itofin library
from itofin import ItofinError
from itofin.quotes import CompositeOp, CompositeQuote, DerivedQuote, SimpleQuote
from itofin.risk import SensitivityAnalysis, parameter_sensitivity, parameter_sensitivity_vector

X = 0.75
H = 0.0625


def _fixture():
    quote = SimpleQuote(X)
    return quote, [CompositeQuote(quote, quote, CompositeOp.Multiply), DerivedQuote(quote, 3.0)]


def test_single_form_matches_the_analytic_oracle():
    quote, parameters = _fixture()
    assert parameter_sensitivity(quote, parameters, shift=H) == [(1.5, 2.0), (3.0, 0.0)]
    one_side = parameter_sensitivity(quote, parameters, shift=H, kind=SensitivityAnalysis.OneSide)
    assert one_side == [(1.5625, None), (3.0, None)]
    assert quote.value() == X


def test_a_missing_reference_value_skips_its_slot():
    quote, parameters = _fixture()
    assert parameter_sensitivity(quote, parameters, [X * X, None], H) == [(1.5, 2.0), (None, None)]
    assert quote.value() == X


def test_a_simple_quote_is_a_parameter():
    quote = SimpleQuote(X)
    assert parameter_sensitivity(quote, [quote, SimpleQuote(1.0)], shift=H) == [(1.0, 0.0), (0.0, 0.0)]


def test_vector_form_is_one_single_form_row_per_quote():
    """Quotes x = 0.75, y = 0.5 against x * x, x - y and x * y, centered.

    d/dx gives (2x, 2), (1, 0), (y, 0) and d/dy gives (0, 0), (-1, 0), (x, 0),
    so the 2 x 3 rows are asymmetric and a transposed layout fails.
    """
    q1, q2 = SimpleQuote(X), SimpleQuote(0.5)
    parameters = [
        CompositeQuote(q1, q1, CompositeOp.Multiply),
        CompositeQuote(q1, q2, CompositeOp.Subtract),
        CompositeQuote(q1, q2, CompositeOp.Multiply),
    ]
    rows = parameter_sensitivity_vector([q1, q2], parameters, H)
    assert rows == [[(1.5, 2.0), (1.0, 0.0), (0.5, 0.0)], [(0.0, 0.0), (-1.0, 0.0), (X, 0.0)]]
    assert rows == [parameter_sensitivity(q, parameters, shift=H) for q in (q1, q2)]
    assert (q1.value(), q2.value()) == (X, 0.5)


def test_a_zero_reference_value_is_a_value():
    """For x * x at x = 0.75, h = 0.0625, a 0.0 reference gives centered second
    ((x + h)^2 + (x - h)^2) / h^2 = 290 and one-sided first (x + h)^2 / h = 10.5625."""
    quote, parameters = _fixture()
    assert parameter_sensitivity(quote, parameters, [0.0, None], H) == [(1.5, 290.0), (None, None)]
    one_side = parameter_sensitivity(quote, parameters, [0.0, None], H, SensitivityAnalysis.OneSide)
    assert one_side == [(10.5625, None), (None, None)]
    assert quote.value() == X


@pytest.mark.parametrize("bad", [math.nan, math.inf, -math.inf])
def test_non_finite_shifts_and_reference_values_are_errors(bad):
    quote, parameters = _fixture()
    with pytest.raises(ItofinError, match="non-finite rate or amount"):
        parameter_sensitivity(quote, parameters, shift=bad)
    with pytest.raises(ItofinError, match="non-finite rate or amount"):
        parameter_sensitivity(quote, parameters, [bad, None], H)
    with pytest.raises(ItofinError, match="non-finite rate or amount"):
        parameter_sensitivity_vector([quote], parameters, bad)
    assert quote.value() == X


def test_invalid_inputs_are_errors_that_leave_the_quote():
    quote, parameters = _fixture()
    with pytest.raises(ItofinError, match="zero shift not allowed"):
        parameter_sensitivity(quote, parameters, shift=0.0)
    with pytest.raises(ItofinError, match="empty parameters vector"):
        parameter_sensitivity(quote, [], shift=H)
    with pytest.raises(ItofinError, match="referenceValues has size 1, instead of 2"):
        parameter_sensitivity(quote, parameters, [None], H)
    with pytest.raises(ItofinError, match="empty SimpleQuote vector"):
        parameter_sensitivity_vector([], parameters, H)
    with pytest.raises(TypeError):
        parameter_sensitivity(cast(Any, parameters[0]), parameters, shift=H)
    assert quote.value() == X
