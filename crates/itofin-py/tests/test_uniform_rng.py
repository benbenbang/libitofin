"""Oracle for the uniform random-number facades in itofin.randomnumbers.

The raw 32-bit stream is pinned against the reference MT19937 ``init_by_array``
output (mt19937ar.c, seeds 0x123 0x234 0x345 0x456) and the ten-thousandth
draw for the default seed 5489, the same values the core pins in
crates/libitofin/src/math/randomnumbers/mt19937uniformrng.rs; the NumPy batch
path returns exactly what the per-draw calls would.
"""

# pypi/conda library
import numpy as np
import pytest

# itofin library
from itofin import ItofinError
from itofin.randomnumbers import UniformRandomGenerator

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


def test_batch_counts_past_the_address_space_are_rejected():
    with pytest.raises(ItofinError):
        UniformRandomGenerator(42).next_reals(2**62)
