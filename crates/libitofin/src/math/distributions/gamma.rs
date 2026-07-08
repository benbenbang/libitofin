//! Gamma distribution.
//!
//! Port of `CumulativeGammaDistribution` from
//! `ql/math/distributions/gammadistribution.{hpp,cpp}`: the CDF of the
//! Gamma(`a`, 1) distribution. QuantLib inlines its own (looser, 3e-7) copy of
//! the incomplete-gamma series/continued-fraction here; we instead delegate to
//! the shared [`incomplete_gamma`] building block (`F(x) = P(a, x)`), which is
//! the same mathematics with tighter accuracy.

use super::Cdf;
use crate::errors::QlResult;
use crate::math::incompletegamma::incomplete_gamma;
use crate::require;
use crate::types::Real;

/// The cumulative distribution function of the Gamma(`a`, 1) distribution.
#[derive(Clone, Copy, Debug)]
pub struct CumulativeGammaDistribution {
    a: Real,
}

impl CumulativeGammaDistribution {
    /// A gamma CDF with shape parameter `a`.
    ///
    /// # Errors
    ///
    /// Returns an error unless `a` is finite and `> 0` (so `NaN` and the
    /// infinities are rejected).
    pub fn new(a: Real) -> QlResult<Self> {
        require!(
            a.is_finite() && a > 0.0,
            "gamma distribution shape must be a finite positive number, got {a}"
        );
        Ok(CumulativeGammaDistribution { a })
    }
}

impl Cdf for CumulativeGammaDistribution {
    fn cdf(&self, x: Real) -> Real {
        if x.is_nan() {
            return Real::NAN;
        }
        // The support is [0, ∞): 0 at/below the lower bound, 1 in the limit at
        // +∞. The +∞ case must be handled here - incomplete_gamma cannot
        // converge for infinite x and would panic the expect below.
        if x <= 0.0 {
            return 0.0;
        }
        if x.is_infinite() {
            return 1.0;
        }
        // a > 0 (constructor invariant) and finite x > 0 make incomplete_gamma
        // valid; its only residual failure is non-convergence, a defect we
        // assert away rather than propagate (see the eval error-boundary convention).
        incomplete_gamma(self.a, x)
            .expect("incomplete_gamma converges for valid gamma-distribution parameters")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::incompletegamma::incomplete_gamma;

    fn assert_close(got: Real, expected: Real, tol: Real) {
        assert!(
            (got - expected).abs() <= tol,
            "got {got}, expected {expected}, diff {}",
            (got - expected).abs()
        );
    }

    #[test]
    fn boundary_values_at_support_edges() {
        let dist = CumulativeGammaDistribution::new(2.5).unwrap();
        assert_eq!(dist.cdf(0.0), 0.0);
        assert_eq!(dist.cdf(-1.0), 0.0);
        assert_eq!(dist.cdf(Real::NEG_INFINITY), 0.0);
        // +∞ is the upper limit of the support: cdf -> 1 (must not panic).
        assert_eq!(dist.cdf(Real::INFINITY), 1.0);
    }

    #[test]
    fn a_eq_1_is_exponential_cdf() {
        // Gamma(1, 1) is Exponential(1): F(x) = 1 - e^{-x}.
        let dist = CumulativeGammaDistribution::new(1.0).unwrap();
        for x in [0.1, 1.0, 2.0, 5.0, 20.0_f64] {
            assert_close(dist.cdf(x), 1.0 - (-x).exp(), 1e-12);
        }
    }

    #[test]
    fn agrees_with_incomplete_gamma_for_positive_x() {
        for a in [0.5, 2.0, 7.5] {
            let dist = CumulativeGammaDistribution::new(a).unwrap();
            for x in [0.25, 1.0, 3.0, 10.0] {
                assert_eq!(dist.cdf(x), incomplete_gamma(a, x).unwrap());
            }
        }
    }

    #[test]
    fn stays_in_unit_interval_and_increases() {
        let dist = CumulativeGammaDistribution::new(3.0).unwrap();
        let mut prev = 0.0;
        let mut x = 0.0;
        while x < 40.0 {
            x += 0.1;
            let p = dist.cdf(x);
            assert!((0.0..=1.0).contains(&p), "cdf({x}) = {p}");
            assert!(p >= prev, "not increasing at x={x}: {prev} -> {p}");
            prev = p;
        }
    }

    #[test]
    fn nan_x_is_nan() {
        assert!(
            CumulativeGammaDistribution::new(2.0)
                .unwrap()
                .cdf(Real::NAN)
                .is_nan()
        );
    }

    #[test]
    fn new_rejects_nonpositive_nan_and_infinite_shape() {
        assert!(CumulativeGammaDistribution::new(0.0).is_err());
        assert!(CumulativeGammaDistribution::new(-1.0).is_err());
        assert!(CumulativeGammaDistribution::new(Real::NAN).is_err());
        assert!(CumulativeGammaDistribution::new(Real::INFINITY).is_err());
    }
}
