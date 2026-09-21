//! Poisson pseudo-random factories with explicit per-generator parameters.
//!
//! Poisson quantiles can fail numerically, so the sequence factory intentionally
//! returns a fallible generator instead of implementing the infallible MC policy.

use super::inversecumulativerng::{FallibleInverseCumulativeRsg, InverseCumulativeRng};
use super::{MersenneTwisterUniformRng, RandomSequenceGenerator};
use crate::errors::QlResult;
use crate::math::distributions::poisson::InverseCumulativePoisson;
use crate::types::Real;

/// Scalar Poisson variates driven by MT19937.
pub type PoissonRng = InverseCumulativeRng<MersenneTwisterUniformRng, InverseCumulativePoisson>;
/// Weighted Poisson sequences driven by MT19937.
pub type PoissonRsg = FallibleInverseCumulativeRsg<
    RandomSequenceGenerator<MersenneTwisterUniformRng>,
    InverseCumulativePoisson,
>;

/// Default rate-one Poisson policy, with explicit custom-rate factories.
pub struct PoissonPseudoRandom;

impl PoissonPseudoRandom {
    /// Construct a rate-one sequence, matching QuantLib's default distribution.
    pub fn make_sequence_generator(dimension: usize, seed: u32) -> QlResult<PoissonRsg> {
        Self::with_lambda(dimension, seed, 1.0)
    }

    /// Construct a sequence with an explicit finite positive Poisson rate.
    ///
    /// # Errors
    /// Rejects zero dimensions and rates unsupported by the quantile recurrence.
    pub fn with_lambda(dimension: usize, seed: u32, lambda: Real) -> QlResult<PoissonRsg> {
        Ok(FallibleInverseCumulativeRsg::new(
            RandomSequenceGenerator::with_seed(dimension, seed)?,
            InverseCumulativePoisson::new(lambda)?,
        ))
    }

    /// Construct a scalar generator with an explicit Poisson rate.
    pub fn make_scalar_generator(seed: u32, lambda: Real) -> QlResult<PoissonRng> {
        Ok(InverseCumulativeRng::new(
            MersenneTwisterUniformRng::new(seed),
            InverseCumulativePoisson::new(lambda)?,
        ))
    }
}
