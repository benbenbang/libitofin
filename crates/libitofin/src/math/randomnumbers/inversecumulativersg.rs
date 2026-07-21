//! Inverse cumulative random sequence generator.
//!
//! Port of `ql/math/randomnumbers/inversecumulativersg.hpp`: draws a uniform
//! sequence from an inner [`SequenceGenerator`] and maps each component through
//! an [`InverseCumulative`] transform, yielding a sequence of distribution
//! deviates. The path weight is copied straight from the uniform sample
//! (`inversecumulativersg.hpp:88`).
//!
//! Divergence: QuantLib's single-argument constructor default-constructs the
//! inverse cumulative (`inversecumulativersg.hpp:60`); here [`new`] always takes
//! it explicitly, since the transform types on main carry no `Default`.

use super::rngtraits::{InverseCumulative, SequenceGenerator};
use crate::methods::montecarlo::Sample;
use crate::types::Real;

/// Maps a uniform [`SequenceGenerator`] `USG` through an inverse cumulative
/// transform `IC`.
pub struct InverseCumulativeRsg<USG, IC> {
    uniform_generator: USG,
    dimension: usize,
    x: Sample<Vec<Real>>,
    inverse_cumulative: IC,
}

impl<USG: SequenceGenerator, IC: InverseCumulative> InverseCumulativeRsg<USG, IC> {
    /// A generator drawing uniforms from `uniform_generator` and mapping them
    /// through `inverse_cumulative`.
    pub fn new(uniform_generator: USG, inverse_cumulative: IC) -> Self {
        let dimension = uniform_generator.dimension();
        InverseCumulativeRsg {
            uniform_generator,
            dimension,
            x: Sample::new(vec![0.0; dimension], 1.0),
            inverse_cumulative,
        }
    }
}

impl<USG: SequenceGenerator, IC: InverseCumulative> SequenceGenerator
    for InverseCumulativeRsg<USG, IC>
{
    fn next_sequence(&mut self) -> &Sample<Vec<Real>> {
        let sample = self.uniform_generator.next_sequence();
        self.x.weight = sample.weight;
        for i in 0..self.dimension {
            self.x.value[i] = self.inverse_cumulative.evaluate(sample.value[i]);
        }
        &self.x
    }

    fn last_sequence(&self) -> &Sample<Vec<Real>> {
        &self.x
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::distributions::normal::InverseCumulativeNormal;
    use crate::math::randomnumbers::randomsequencegenerator::RandomSequenceGenerator;

    // A sequence generator returning a preset weighted sample, so a test can
    // pin the transform and weight-propagation paths against known inputs.
    struct FixedSequenceGenerator {
        sample: Sample<Vec<Real>>,
    }

    impl SequenceGenerator for FixedSequenceGenerator {
        fn next_sequence(&mut self) -> &Sample<Vec<Real>> {
            &self.sample
        }
        fn last_sequence(&self) -> &Sample<Vec<Real>> {
            &self.sample
        }
        fn dimension(&self) -> usize {
            self.sample.value.len()
        }
    }

    #[test]
    fn maps_a_known_uniform_vector_to_the_inverse_normal() {
        let uniforms = vec![0.1, 0.25, 0.5, 0.75, 0.9];
        let usg = FixedSequenceGenerator {
            sample: Sample::new(uniforms.clone(), 1.0),
        };
        let mut rsg = InverseCumulativeRsg::new(usg, InverseCumulativeNormal::standard());
        let out = rsg.next_sequence();
        for (i, &u) in uniforms.iter().enumerate() {
            let expected = InverseCumulativeNormal::standard().value(u).unwrap();
            assert!(
                (out.value[i] - expected).abs() < 1e-15,
                "component {i}: got {}, want {expected}",
                out.value[i]
            );
        }
    }

    #[test]
    fn cross_checks_the_real_mersenne_twister_chain() {
        let mut driven = InverseCumulativeRsg::new(
            RandomSequenceGenerator::with_seed(4, 42).unwrap(),
            InverseCumulativeNormal::standard(),
        );
        let mut mirror = RandomSequenceGenerator::with_seed(4, 42).unwrap();
        let gaussians = driven.next_sequence().value.clone();
        let uniforms = mirror.next_sequence().value.clone();
        for (g, u) in gaussians.iter().zip(&uniforms) {
            let expected = InverseCumulativeNormal::standard().value(*u).unwrap();
            assert!((g - expected).abs() < 1e-15, "got {g}, want {expected}");
        }
    }

    #[test]
    fn propagates_the_uniform_sample_weight() {
        let usg = FixedSequenceGenerator {
            sample: Sample::new(vec![0.3, 0.6], 0.25),
        };
        let mut rsg = InverseCumulativeRsg::new(usg, InverseCumulativeNormal::standard());
        assert_eq!(rsg.next_sequence().weight, 0.25);
    }

    #[test]
    fn reports_the_inner_dimension() {
        let rsg = InverseCumulativeRsg::new(
            RandomSequenceGenerator::with_seed(7, 1).unwrap(),
            InverseCumulativeNormal::standard(),
        );
        assert_eq!(rsg.dimension(), 7);
    }
}
