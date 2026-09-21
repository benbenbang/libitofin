//! Fallible scalar and sequence inverse-cumulative generators.
//!
//! Explicit transforms replace QuantLib's global inverse-cumulative override.
//! Errors consume the attempted draw and preserve the last successful sequence.

use super::UniformRng;
use super::rngtraits::SequenceGenerator;
use crate::errors::QlResult;
use crate::math::distributions::normal::InverseCumulativeNormal;
use crate::math::distributions::poisson::InverseCumulativePoisson;
use crate::math::distributions::{Probability, Quantile};
use crate::methods::montecarlo::Sample;
use crate::require;
use crate::types::Real;

/// An inverse transform with explicit domain and numerical errors.
pub trait FallibleInverseCumulative {
    /// Transform a uniform draw, returning an error for invalid or unresolved input.
    fn try_evaluate(&self, uniform: Real) -> QlResult<Real>;
}

impl FallibleInverseCumulative for InverseCumulativeNormal {
    fn try_evaluate(&self, uniform: Real) -> QlResult<Real> {
        require!(
            uniform > 0.0 && uniform < 1.0,
            "normal uniform must lie in (0, 1)"
        );
        self.value(uniform)
    }
}

impl FallibleInverseCumulative for InverseCumulativePoisson {
    fn try_evaluate(&self, uniform: Real) -> QlResult<Real> {
        require!(
            (0.0..1.0).contains(&uniform),
            "Poisson uniform must lie in [0, 1)"
        );
        self.quantile(Probability::try_from(uniform)?)
    }
}

/// Maps a scalar uniform generator through an explicit inverse cumulative.
#[derive(Clone)]
pub struct InverseCumulativeRng<R, IC> {
    uniform: R,
    inverse: IC,
}

impl<R: UniformRng, IC: FallibleInverseCumulative> InverseCumulativeRng<R, IC> {
    /// Own the supplied generator state and inverse transform.
    pub fn new(uniform: R, inverse: IC) -> Self {
        Self { uniform, inverse }
    }

    /// Draw a transformed scalar with unit weight.
    ///
    /// # Errors
    /// Returns transform domain or numerical errors without panicking.
    pub fn next_sample(&mut self) -> QlResult<Sample<Real>> {
        Ok(Sample::new(
            self.inverse.try_evaluate(self.uniform.next_real())?,
            1.0,
        ))
    }
}

/// Fallible counterpart of `InverseCumulativeRsg`, preserving source weights.
#[derive(Clone)]
pub struct FallibleInverseCumulativeRsg<USG, IC> {
    uniform: USG,
    inverse: IC,
    last: Sample<Vec<Real>>,
}

impl<USG: SequenceGenerator, IC: FallibleInverseCumulative> FallibleInverseCumulativeRsg<USG, IC> {
    /// Own the supplied sequence generator and inverse transform.
    pub fn new(uniform: USG, inverse: IC) -> Self {
        let last = Sample::new(vec![0.0; uniform.dimension()], 1.0);
        Self {
            uniform,
            inverse,
            last,
        }
    }

    /// Draw and transform a sequence, preserving the last success on error.
    ///
    /// # Errors
    /// Returns any component's transform error.
    pub fn next_sequence(&mut self) -> QlResult<&Sample<Vec<Real>>> {
        let sample = self.uniform.next_sequence();
        let value = sample
            .value
            .iter()
            .map(|&x| self.inverse.try_evaluate(x))
            .collect::<QlResult<Vec<_>>>()?;
        self.last = Sample::new(value, sample.weight);
        Ok(&self.last)
    }

    /// Most recent successful sample, initially zeros with unit weight.
    pub fn last_sequence(&self) -> &Sample<Vec<Real>> {
        &self.last
    }

    /// Number of components in each sequence.
    pub fn dimension(&self) -> usize {
        self.uniform.dimension()
    }
}
