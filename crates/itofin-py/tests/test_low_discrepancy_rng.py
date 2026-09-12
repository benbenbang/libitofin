"""Oracle for the low-discrepancy facades in itofin.randomnumbers.

The first Sobol dimension is pinned to the van der Corput sequence modulo two,
the values test-suite/lowdiscrepancysequences.cpp pins and the core repeats in
crates/libitofin/src/math/randomnumbers/sobol/mod.rs.
The Sobol homogeneity check (the mean of each dimension is exactly 0.5 at the
end of every 2^j - 1 cycle) is testSobol's.
"""

# pypi/conda library
import numpy as np
import pytest

# itofin library
from itofin import ItofinError
from itofin.randomnumbers import DirectionIntegers, SobolRsg

VAN_DER_CORPUT_SOBOL = [
    0.50000, 0.75000, 0.25000, 0.37500, 0.87500, 0.62500, 0.12500, 0.18750, 0.68750, 0.93750,
    0.43750, 0.31250, 0.81250, 0.56250, 0.06250, 0.09375, 0.59375, 0.84375, 0.34375, 0.46875,
    0.96875, 0.71875, 0.21875, 0.15625, 0.65625, 0.90625, 0.40625, 0.28125, 0.78125, 0.53125,
    0.03125,
]  # fmt: skip



TOLERANCE = 1e-15


def test_sobol_first_dimension_is_the_van_der_corput_sequence():
    rsg = SobolRsg(1)
    assert rsg.dimension() == 1
    for expected in VAN_DER_CORPUT_SOBOL:
        point = rsg.next_sequence()
        assert point.dtype == np.float64
        assert point.shape == (1,)
        assert abs(point[0] - expected) <= TOLERANCE
        assert rsg.last_sequence().tolist() == point.tolist()


def test_sobol_homogeneity_over_33_dimensions():
    dimensionality = 33
    rsg = SobolRsg(dimensionality, 123456, DirectionIntegers.Jaeckel)
    sums = np.zeros(dimensionality)
    k = 0
    for j in range(1, 5):
        points = 2**j - 1
        while k < points:
            sums += rsg.next_sequence()
            k += 1
        assert np.all(np.abs(sums / k - 0.5) <= TOLERANCE)


def test_sobol_next_sequences_stacks_successive_draws():
    single = SobolRsg(5, 7)
    matrix = SobolRsg(5, 7).next_sequences(40)
    assert matrix.shape == (40, 5)
    assert matrix.flags["C_CONTIGUOUS"]
    for row in matrix:
        assert row.tolist() == single.next_sequence().tolist()
    assert np.all((matrix > 0.0) & (matrix < 1.0))


def test_sobol_int32_sequence_is_the_float_point_scaled():
    ints = SobolRsg(4)
    floats = SobolRsg(4)
    for _ in range(16):
        raw = ints.next_int32_sequence()
        assert raw.dtype == np.uint32
        assert np.array_equal(raw.astype(np.float64) / 2.0**32, floats.next_sequence())


def test_sobol_skip_to_returns_the_point_and_positions_the_counter():
    points = SobolRsg(2).next_sequences(8).tolist()
    fresh = SobolRsg(2)
    skipped = fresh.skip_to(3)
    assert skipped.dtype == np.uint32
    assert (skipped.astype(np.float64) / 2.0**32).tolist() == points[3]
    # Before any draw the skipped-to point itself is returned; afterwards the
    # draw advances past it, matching QuantLib's Gray-code counter semantics.
    assert fresh.next_sequence().tolist() == points[3]
    assert fresh.next_sequence().tolist() == points[4]
    drawn = SobolRsg(2)
    drawn.next_sequence()
    assert (drawn.skip_to(3).astype(np.float64) / 2.0**32).tolist() == points[3]
    assert drawn.next_sequence().tolist() == points[4]


def test_sobol_skip_to_with_the_plain_counter_redraws_the_point():
    plain = SobolRsg(2, 0, DirectionIntegers.Jaeckel, False)
    draws = [plain.next_sequence().tolist() for _ in range(8)]
    skipped = (plain.skip_to(3).astype(np.float64) / 2.0**32).tolist()
    # The plain counter re-reads the skipped-to point on the next draw, then
    # moves on; the point sits where the generator's own draws placed it.
    assert plain.next_sequence().tolist() == skipped
    assert plain.next_sequence().tolist() == draws[draws.index(skipped) + 1]


def test_sobol_direction_integer_tables_all_build_and_share_the_first_dimension():
    for table in (
        DirectionIntegers.Unit,
        DirectionIntegers.Jaeckel,
        DirectionIntegers.SobolLevitan,
        DirectionIntegers.SobolLevitanLemieux,
        DirectionIntegers.JoeKuoD5,
        DirectionIntegers.JoeKuoD6,
        DirectionIntegers.JoeKuoD7,
        DirectionIntegers.Kuo,
        DirectionIntegers.Kuo2,
        DirectionIntegers.Kuo3,
    ):
        rsg = SobolRsg(8, 0, table)
        first = rsg.next_sequences(8)[:, 0].tolist()
        assert first == VAN_DER_CORPUT_SOBOL[:8]


def test_sobol_default_table_is_jaeckel():
    default = SobolRsg(16).next_sequences(32)
    jaeckel = SobolRsg(16, 0, DirectionIntegers.Jaeckel).next_sequences(32)
    assert np.array_equal(default, jaeckel)


def test_sobol_rejects_dimensions_outside_the_polynomial_table():
    with pytest.raises(ItofinError):
        SobolRsg(0)
    with pytest.raises(ItofinError):
        SobolRsg(21201)
    with pytest.raises(ItofinError):
        SobolRsg(1).skip_to(2**32 - 1)


def test_batch_counts_past_the_address_space_are_rejected():
    with pytest.raises(ItofinError):
        SobolRsg(3).next_sequences(2**63)
