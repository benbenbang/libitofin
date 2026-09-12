"""Oracle for the Gaussian random-number facades in itofin.randomnumbers.

The Box-Muller stream over seed 42 is pinned to the QuantLib values the core
pins in crates/libitofin/src/math/randomnumbers/boxmullergaussianrng.rs, and
the moments of a long stream are standard normal.
"""

# pypi/conda library
import numpy as np
import pytest

# itofin library
from itofin import ItofinError
from itofin.randomnumbers import GaussianRandomGenerator, UniformRandomGenerator

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


def test_batch_counts_past_the_address_space_are_rejected():
    with pytest.raises(ItofinError):
        GaussianRandomGenerator.with_seed(42).next_gaussians(2**62)
