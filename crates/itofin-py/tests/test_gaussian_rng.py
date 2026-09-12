"""Oracle for the Gaussian random-number facades in itofin.randomnumbers.

The Box-Muller stream over seed 42 is pinned to the QuantLib values the core
pins in crates/libitofin/src/math/randomnumbers/boxmullergaussianrng.rs. The
inverse-cumulative sequence generator is cross-checked against the uniform
sequence it maps: the standard normal CDF of every Gaussian draw, computed
here through math.erfc, recovers the uniform draw to the accuracy of Acklam's
inverse (about 1e-9 relative), and the moments of a long stream are standard
normal.
"""

# standard library
import math

# pypi/conda library
import numpy as np
import pytest

# itofin library
from itofin import ItofinError
from itofin.randomnumbers import (
    GaussianRandomGenerator,
    GaussianRandomSequenceGenerator,
    UniformRandomGenerator,
    UniformRandomSequenceGenerator,
)

BOX_MULLER_SEED_42 = [
    -0.51696416445487181,
    1.2219212173764127,
    0.72133261267083881,
    0.86963581617716534,
    1.6182168832131514,
    1.5885563656499377,
    -1.1883085743351087,
    -0.18712466949524548,
]


def _normal_cdf(x):
    return 0.5 * math.erfc(-x / math.sqrt(2.0))


def test_box_muller_matches_the_quantlib_stream():
    rng = GaussianRandomGenerator(UniformRandomGenerator(42))
    assert [rng.next_gaussian() for _ in range(8)] == BOX_MULLER_SEED_42


def test_with_seed_matches_the_two_step_construction():
    assert GaussianRandomGenerator.with_seed(42).next_gaussians(8).tolist() == BOX_MULLER_SEED_42


def test_next_gaussians_batches_the_scalar_stream():
    scalar = GaussianRandomGenerator.with_seed(1234)
    batch = GaussianRandomGenerator.with_seed(1234).next_gaussians(101)
    assert batch.dtype == np.float64
    assert batch.shape == (101,)
    assert batch.tolist() == [scalar.next_gaussian() for _ in range(101)]


def test_box_muller_moments_are_standard_normal():
    draws = GaussianRandomGenerator.with_seed(1234).next_gaussians(100_000)
    assert abs(draws.mean()) < 0.01
    assert abs(draws.var() - 1.0) < 0.01


def test_gaussian_generator_copies_the_uniform_generator():
    rng = UniformRandomGenerator(42)
    gaussian = GaussianRandomGenerator(rng)
    gaussian.next_gaussians(8)
    # The original uniform generator was left where it stood.
    assert rng.next_real() == UniformRandomGenerator(42).next_real()


def test_sequence_generator_inverts_the_uniform_sequence():
    usg = UniformRandomSequenceGenerator(6, UniformRandomGenerator(42))
    gsg = GaussianRandomSequenceGenerator(usg)
    assert gsg.dimension() == 6
    for _ in range(20):
        uniforms = usg.next_sequence()
        gaussians = gsg.next_sequence()
        assert gaussians.dtype == np.float64
        assert gaussians.shape == (6,)
        for z, u in zip(gaussians.tolist(), uniforms.tolist()):
            assert abs(_normal_cdf(z) - u) < 1e-8
        assert gsg.last_sequence().tolist() == gaussians.tolist()


def test_with_seed_matches_the_two_step_sequence_construction():
    direct = GaussianRandomSequenceGenerator.with_seed(4, 42)
    two_step = GaussianRandomSequenceGenerator(UniformRandomSequenceGenerator.with_seed(4, 42))
    for _ in range(5):
        assert direct.next_sequence().tolist() == two_step.next_sequence().tolist()


def test_next_sequences_stacks_successive_draws_row_by_row():
    single = GaussianRandomSequenceGenerator.with_seed(3, 7)
    matrix = GaussianRandomSequenceGenerator.with_seed(3, 7).next_sequences(50)
    assert matrix.shape == (50, 3)
    assert matrix.flags["C_CONTIGUOUS"]
    for row in matrix:
        assert row.tolist() == single.next_sequence().tolist()


def test_sequence_moments_are_standard_normal():
    draws = GaussianRandomSequenceGenerator.with_seed(10, 99).next_sequences(10_000)
    assert abs(draws.mean()) < 0.01
    assert abs(draws.var() - 1.0) < 0.02


def test_same_seed_reproduces_and_another_seed_differs():
    a = GaussianRandomSequenceGenerator.with_seed(4, 42).next_sequences(3)
    b = GaussianRandomSequenceGenerator.with_seed(4, 42).next_sequences(3)
    c = GaussianRandomSequenceGenerator.with_seed(4, 43).next_sequences(3)
    assert a.tolist() == b.tolist()
    assert a.tolist() != c.tolist()


def test_last_sequence_is_zero_before_the_first_draw():
    assert GaussianRandomSequenceGenerator.with_seed(4, 42).last_sequence().tolist() == [0.0] * 4


def test_zero_dimension_is_rejected():
    with pytest.raises(ItofinError):
        GaussianRandomSequenceGenerator.with_seed(0, 42)


def test_batch_counts_past_the_address_space_are_rejected():
    with pytest.raises(ItofinError):
        GaussianRandomGenerator.with_seed(42).next_gaussians(2**62)
    with pytest.raises(ItofinError):
        GaussianRandomSequenceGenerator.with_seed(3, 42).next_sequences(2**63)
