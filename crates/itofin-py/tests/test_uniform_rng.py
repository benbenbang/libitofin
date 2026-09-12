"""Oracle for the uniform random-number facades in itofin.randomnumbers.

The raw 32-bit stream is pinned against the reference MT19937 ``init_by_array``
output (mt19937ar.c, seeds 0x123 0x234 0x345 0x456) and the ten-thousandth
draw for the default seed 5489, the same values the core pins in
crates/libitofin/src/math/randomnumbers/mt19937uniformrng.rs. The sequence
generator is checked for consistency with the scalar generator it copies, so
the NumPy batch paths return exactly what the per-draw calls would.
"""

# pypi/conda library
import numpy as np
import pytest

# itofin library
from itofin import ItofinError
from itofin.randomnumbers import UniformRandomGenerator, UniformRandomSequenceGenerator

INIT_BY_ARRAY_REFERENCE = [
    1067595299,
    955945823,
    477289528,
    4107218783,
    4228976476,
    3344332714,
    3355579695,
    227628506,
    810200273,
    2591290167,
]


def test_from_seeds_matches_the_reference_init_by_array_stream():
    rng = UniformRandomGenerator.from_seeds([0x123, 0x234, 0x345, 0x456])
    assert [rng.next_u32() for _ in range(10)] == INIT_BY_ARRAY_REFERENCE


def test_default_seed_ten_thousandth_draw_matches_the_reference():
    rng = UniformRandomGenerator(5489)
    for _ in range(9999):
        rng.next_u32()
    assert rng.next_u32() == 4123659995


def test_from_seeds_rejects_an_empty_array():
    with pytest.raises(ItofinError):
        UniformRandomGenerator.from_seeds([])


def test_next_real_is_the_shifted_scaled_u32_inside_the_open_unit_interval():
    words = UniformRandomGenerator(42)
    reals = UniformRandomGenerator(42)
    for _ in range(1000):
        expected = (words.next_u32() + 0.5) / 4294967296.0
        x = reals.next_real()
        assert x == expected
        assert 0.0 < x < 1.0


def test_next_reals_batches_the_scalar_stream_into_a_float64_array():
    scalar = UniformRandomGenerator(42)
    batch = UniformRandomGenerator(42).next_reals(64)
    assert isinstance(batch, np.ndarray)
    assert batch.dtype == np.float64
    assert batch.shape == (64,)
    assert batch.tolist() == [scalar.next_real() for _ in range(64)]


def test_same_seed_reproduces_and_zero_seed_diverges():
    assert UniformRandomGenerator(7).next_reals(8).tolist() == UniformRandomGenerator(7).next_reals(8).tolist()
    assert UniformRandomGenerator(7).next_reals(8).tolist() != UniformRandomGenerator(8).next_reals(8).tolist()
    assert UniformRandomGenerator(0).next_reals(8).tolist() != UniformRandomGenerator(0).next_reals(8).tolist()


def test_sequence_generator_copies_the_scalar_generator():
    rng = UniformRandomGenerator(42)
    usg = UniformRandomSequenceGenerator(3, rng)
    assert usg.dimension() == 3

    first = usg.next_sequence()
    assert isinstance(first, np.ndarray)
    assert first.dtype == np.float64
    assert first.shape == (3,)
    # The sequence generator drew from its own copy: the original is untouched
    # and yields the very same three values.
    assert first.tolist() == [rng.next_real() for _ in range(3)]
    assert usg.last_sequence().tolist() == first.tolist()


def test_with_seed_matches_the_two_step_construction():
    direct = UniformRandomSequenceGenerator.with_seed(4, 42)
    two_step = UniformRandomSequenceGenerator(4, UniformRandomGenerator(42))
    for _ in range(5):
        assert direct.next_sequence().tolist() == two_step.next_sequence().tolist()


def test_next_sequences_stacks_successive_draws_row_by_row():
    single = UniformRandomSequenceGenerator.with_seed(5, 42)
    matrix = UniformRandomSequenceGenerator.with_seed(5, 42).next_sequences(10)
    assert matrix.shape == (10, 5)
    assert matrix.dtype == np.float64
    assert matrix.flags["C_CONTIGUOUS"]
    for row in matrix:
        assert row.tolist() == single.next_sequence().tolist()
    assert np.all((matrix > 0.0) & (matrix < 1.0))


def test_next_sequences_with_zero_count_is_an_empty_matrix():
    usg = UniformRandomSequenceGenerator.with_seed(3, 42)
    empty = usg.next_sequences(0)
    assert empty.shape == (0, 3)
    # Nothing was drawn.
    assert usg.next_sequence().tolist() == UniformRandomSequenceGenerator.with_seed(3, 42).next_sequence().tolist()


def test_last_sequence_is_zero_before_the_first_draw():
    assert UniformRandomSequenceGenerator.with_seed(4, 42).last_sequence().tolist() == [0.0] * 4


def test_zero_dimension_is_rejected():
    with pytest.raises(ItofinError):
        UniformRandomSequenceGenerator(0, UniformRandomGenerator(42))
    with pytest.raises(ItofinError):
        UniformRandomSequenceGenerator.with_seed(0, 42)


def test_batch_counts_past_the_address_space_are_rejected():
    with pytest.raises(ItofinError):
        UniformRandomGenerator(42).next_reals(2**62)
    with pytest.raises(ItofinError):
        UniformRandomSequenceGenerator.with_seed(3, 42).next_sequences(2**63)
