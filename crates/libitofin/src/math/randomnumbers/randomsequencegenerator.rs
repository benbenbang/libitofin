//! Random sequence generator over a scalar uniform RNG.
//!
//! Port of `ql/math/randomnumbers/randomsequencegenerator.hpp`: wraps a scalar
//! [`UniformRng`] and yields `dimension` uniform draws as a weighted
//! [`Sample`].
//!
//! Divergences from `randomsequencegenerator.hpp`:
//! - QuantLib's generic `RNG::next()` returns a weighted `Sample<Real>` whose
//!   weight it multiplies into the sequence weight (:73). The on-main
//!   [`UniformRng`] carries no per-draw weight (it is always `1.0`, as it is for
//!   QuantLib's Mersenne-Twister), so the multiplicative path collapses and the
//!   sequence weight is fixed at `1.0`. Weight *propagation* is still exercised
//!   downstream by
//!   [`InverseCumulativeRsg`](super::inversecumulativersg::InverseCumulativeRsg).
//! - the two C++ constructors are asymmetric: `(dim, rng)` guards `dim > 0`
//!   (:58) while `(dim, seed)` does not (:62). Here [`new`](RandomSequenceGenerator::new)
//!   and [`with_seed`](RandomSequenceGenerator::with_seed) both guard, and the
//!   seed constructor is specialised to [`MersenneTwisterUniformRng`] because a
//!   generic seed constructor would need every RNG to be seed-constructible,
//!   which [`UniformRng`] does not model.
//! - `nextInt32Sequence` (:77) is deferred: the Monte Carlo path never calls it.

use super::UniformRng;
use super::mt19937uniformrng::MersenneTwisterUniformRng;
use super::rngtraits::SequenceGenerator;
use crate::errors::QlResult;
use crate::methods::montecarlo::Sample;
use crate::require;
use crate::types::Real;

/// A random sequence generator wrapping a scalar uniform RNG `R`.
pub struct RandomSequenceGenerator<R> {
    dimension: usize,
    rng: R,
    sequence: Sample<Vec<Real>>,
}

impl<R: UniformRng> RandomSequenceGenerator<R> {
    /// A generator of `dimension`-wide sequences over the given RNG.
    ///
    /// # Errors
    ///
    /// Returns an error if `dimension` is zero (`randomsequencegenerator.hpp:58`).
    pub fn new(dimension: usize, rng: R) -> QlResult<Self> {
        require!(dimension > 0, "dimensionality must be greater than 0");
        Ok(RandomSequenceGenerator {
            dimension,
            rng,
            sequence: Sample::new(vec![0.0; dimension], 1.0),
        })
    }
}

impl RandomSequenceGenerator<MersenneTwisterUniformRng> {
    /// A generator over a Mersenne-Twister seeded with `seed`.
    ///
    /// Mirrors QuantLib's `(dimensionality, seed)` constructor
    /// (`randomsequencegenerator.hpp:62`). A seed of `0` draws a random seed
    /// from the seed generator, per [`MersenneTwisterUniformRng::new`].
    ///
    /// # Errors
    ///
    /// Returns an error if `dimension` is zero.
    pub fn with_seed(dimension: usize, seed: u32) -> QlResult<Self> {
        Self::new(dimension, MersenneTwisterUniformRng::new(seed))
    }
}

impl<R: UniformRng> SequenceGenerator for RandomSequenceGenerator<R> {
    fn next_sequence(&mut self) -> &Sample<Vec<Real>> {
        self.sequence.weight = 1.0;
        for i in 0..self.dimension {
            self.sequence.value[i] = self.rng.next_real();
        }
        &self.sequence
    }

    fn last_sequence(&self) -> &Sample<Vec<Real>> {
        &self.sequence
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_sequence_yields_dimension_open_unit_draws_with_unit_weight() {
        let mut rsg = RandomSequenceGenerator::with_seed(3, 42).unwrap();
        let sample = rsg.next_sequence();
        assert_eq!(sample.value.len(), 3);
        assert_eq!(sample.weight, 1.0);
        for &x in &sample.value {
            assert!(x > 0.0 && x < 1.0, "draw {x} not in (0, 1)");
        }
    }

    #[test]
    fn zero_dimension_is_rejected() {
        assert!(RandomSequenceGenerator::with_seed(0, 42).is_err());
        let rng = MersenneTwisterUniformRng::new(42);
        assert!(RandomSequenceGenerator::new(0, rng).is_err());
    }

    #[test]
    fn last_sequence_returns_the_most_recent_draw() {
        let mut rsg = RandomSequenceGenerator::with_seed(4, 7).unwrap();
        let drawn = rsg.next_sequence().value.clone();
        assert_eq!(rsg.last_sequence().value, drawn);
    }

    #[test]
    fn same_seed_reproduces_the_sequence_across_generators() {
        let mut a = RandomSequenceGenerator::with_seed(5, 42).unwrap();
        let mut b = RandomSequenceGenerator::with_seed(5, 42).unwrap();
        for _ in 0..4 {
            assert_eq!(a.next_sequence().value, b.next_sequence().value);
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = RandomSequenceGenerator::with_seed(5, 42).unwrap();
        let mut c = RandomSequenceGenerator::with_seed(5, 43).unwrap();
        assert_ne!(a.next_sequence().value, c.next_sequence().value);
    }
}
