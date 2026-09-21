"""QuantLib rngtraits.cpp stored Poisson sums and fallible facade contracts."""

import math

import pytest

from itofin import ItofinError
from itofin.randomnumbers import PoissonRandomGenerator, PoissonRandomSequenceGenerator


@pytest.mark.parametrize(("rate", "expected"), [(1.0, 108.0), (4.0, 409.0)])
def test_poisson_quantlib_seed_oracles_and_copies(rate, expected):
    scalar = PoissonRandomGenerator(1234, rate)
    sequence = PoissonRandomSequenceGenerator(100, 1234, rate)
    assert sequence.dimension() == 100
    assert sequence.last_sequence() == [0.0] * 100
    values = sequence.next_sequence()
    assert all(math.isfinite(x) and x >= 0 and x.is_integer() for x in values)
    assert sum(values) == expected
    assert values == [scalar.next_real() for _ in range(100)]
    assert sequence.last_sequence() == values
    values[0] = -1.0
    assert sequence.last_sequence()[0] >= 0
    clone = sequence.copy()
    assert clone.next_sequence() == sequence.next_sequence()
    scalar_copy = scalar.copy()
    assert scalar_copy.next_real() == scalar.next_real()
    del scalar, sequence
    assert math.isfinite(scalar_copy.next_real())
    assert all(math.isfinite(x) for x in clone.next_sequence())


def test_poisson_defaults_and_invalid_input_recovery():
    assert sum(PoissonRandomSequenceGenerator(100, 1234).next_sequence()) == 108.0
    for rate in [0.0, -1.0, math.nan, math.inf, 750.0]:
        with pytest.raises(ItofinError):
            PoissonRandomGenerator(42, rate)
        with pytest.raises(ItofinError):
            PoissonRandomSequenceGenerator(3, 42, rate)
    with pytest.raises(ItofinError):
        PoissonRandomSequenceGenerator(0)
    with pytest.raises(ItofinError):
        PoissonRandomSequenceGenerator(16777217)
    assert math.isfinite(PoissonRandomGenerator(42).next_real())
