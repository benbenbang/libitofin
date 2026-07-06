//! Box-Muller Gaussian random-number generator.
//!
//! Port of `ql/math/randomnumbers/boxmullergaussianrng.hpp`: the polar
//! (rejection) form of the Box-Muller transformation, turning pairs of
//! uniform deviates in `(0, 1)` into pairs of standard normal deviates.
//! QuantLib caches the second deviate of each pair through `mutable`
//! members; here the cache is an `Option`.

#![allow(clippy::excessive_precision)]

use super::{GaussianRng, UniformRng};
use crate::types::Real;

/// Gaussian random number generator drawing its uniform deviates from `U`.
#[derive(Clone)]
pub struct BoxMullerGaussianRng<U: UniformRng> {
    uniform_generator: U,
    second_value: Option<Real>,
}

impl<U: UniformRng> BoxMullerGaussianRng<U> {
    /// A generator consuming uniform deviates from `uniform_generator`.
    pub fn new(uniform_generator: U) -> Self {
        BoxMullerGaussianRng {
            uniform_generator,
            second_value: None,
        }
    }
}

impl<U: UniformRng> GaussianRng for BoxMullerGaussianRng<U> {
    fn next_gaussian(&mut self) -> Real {
        if let Some(second) = self.second_value.take() {
            return second;
        }
        let (x1, x2, r) = loop {
            let x1 = self.uniform_generator.next_real() * 2.0 - 1.0;
            let x2 = self.uniform_generator.next_real() * 2.0 - 1.0;
            let r = x1 * x1 + x2 * x2;
            if r < 1.0 && r != 0.0 {
                break (x1, x2, r);
            }
        };
        let ratio = (-2.0 * r.ln() / r).sqrt();
        self.second_value = Some(x2 * ratio);
        x1 * ratio
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::randomnumbers::MersenneTwisterUniformRng;

    #[test]
    fn matches_quantlib_sequence_over_mt19937() {
        let mut rng = BoxMullerGaussianRng::new(MersenneTwisterUniformRng::new(42));
        let expected = [
            -0.51696416445487181,
            1.2219212173764127,
            0.72133261267083881,
            0.86963581617716534,
            1.6182168832131514,
            1.5885563656499377,
            -1.1883085743351087,
            -0.18712466949524548,
        ];
        for (i, &e) in expected.iter().enumerate() {
            assert_eq!(rng.next_gaussian(), e, "mismatch at index {i}");
        }
    }

    #[test]
    fn sample_moments_are_standard_normal() {
        let mut rng = BoxMullerGaussianRng::new(MersenneTwisterUniformRng::new(1234));
        let n = 100_000;
        let mut sum = 0.0;
        let mut sum_sq = 0.0;
        for _ in 0..n {
            let x = rng.next_gaussian();
            sum += x;
            sum_sq += x * x;
        }
        let mean = sum / n as Real;
        let variance = sum_sq / n as Real - mean * mean;
        assert!(mean.abs() < 0.01, "mean {mean} not close to 0");
        assert!(
            (variance - 1.0).abs() < 0.01,
            "variance {variance} not close to 1"
        );
    }
}
