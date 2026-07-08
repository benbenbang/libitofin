//! Chi-square distribution.
//!
//! Port of `CumulativeChiSquareDistribution` from
//! `ql/math/distributions/chisquaredistribution.{hpp,cpp}`: the (central)
//! chi-square CDF with `df` degrees of freedom, which is exactly the
//! Gamma(`df`/2, 1) CDF evaluated at `x`/2. We delegate to
//! [`CumulativeGammaDistribution`], matching QuantLib.

use super::Cdf;
use super::gamma::CumulativeGammaDistribution;
use crate::errors::QlResult;
use crate::require;
use crate::types::Real;

/// The cumulative distribution function of the (central) chi-square
/// distribution with `df` degrees of freedom.
#[derive(Clone, Copy, Debug)]
pub struct CumulativeChiSquareDistribution {
    gamma: CumulativeGammaDistribution,
}

impl CumulativeChiSquareDistribution {
    /// A chi-square CDF with `df` degrees of freedom.
    ///
    /// QuantLib does not validate `df` in the constructor and instead relies on
    /// the underlying gamma constructor (`df`/2 > 0); we validate up front for a
    /// clear chi-square-specific error, consistent with the finite-positive
    /// convention used elsewhere.
    ///
    /// # Errors
    ///
    /// Returns an error unless `df` is finite and `> 0`.
    pub fn new(df: Real) -> QlResult<Self> {
        require!(
            df.is_finite() && df > 0.0,
            "chi-square degrees of freedom must be a finite positive number, got {df}"
        );
        Ok(CumulativeChiSquareDistribution {
            gamma: CumulativeGammaDistribution::new(0.5 * df)?,
        })
    }
}

impl Cdf for CumulativeChiSquareDistribution {
    fn cdf(&self, x: Real) -> Real {
        // central chi-square CDF = Gamma(df/2, 1) CDF at x/2
        self.gamma.cdf(0.5 * x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(got: Real, expected: Real, tol: Real) {
        assert!(
            (got - expected).abs() <= tol,
            "got {got}, expected {expected}, diff {}",
            (got - expected).abs()
        );
    }

    #[test]
    fn boundary_values_at_support_edges() {
        let dist = CumulativeChiSquareDistribution::new(3.0).unwrap();
        assert_eq!(dist.cdf(0.0), 0.0);
        assert_eq!(dist.cdf(-2.0), 0.0);
        // +∞ delegates to the gamma CDF's upper limit: cdf -> 1 (must not panic).
        assert_eq!(dist.cdf(Real::INFINITY), 1.0);
    }

    #[test]
    fn df_eq_2_is_exponential() {
        // Chi-square with 2 dof is Exponential(mean 2): F(x) = 1 - e^{-x/2}.
        let dist = CumulativeChiSquareDistribution::new(2.0).unwrap();
        for x in [0.5, 1.0, 2.0, 5.0, 20.0_f64] {
            assert_close(dist.cdf(x), 1.0 - (-0.5 * x).exp(), 1e-12);
        }
    }

    #[test]
    fn agrees_with_gamma_at_half_arguments() {
        for df in [1.0, 4.0, 9.0] {
            let chi = CumulativeChiSquareDistribution::new(df).unwrap();
            let gamma = CumulativeGammaDistribution::new(0.5 * df).unwrap();
            for x in [0.5, 2.0, 7.0, 15.0] {
                assert_eq!(chi.cdf(x), gamma.cdf(0.5 * x));
            }
        }
    }

    #[test]
    fn stays_in_unit_interval_and_increases() {
        let dist = CumulativeChiSquareDistribution::new(5.0).unwrap();
        let mut prev = 0.0;
        let mut x = 0.0;
        while x < 50.0 {
            x += 0.1;
            let p = dist.cdf(x);
            assert!((0.0..=1.0).contains(&p), "cdf({x}) = {p}");
            assert!(p >= prev, "not increasing at x={x}: {prev} -> {p}");
            prev = p;
        }
    }

    #[test]
    fn large_df_near_mean_does_not_panic() {
        // Regression (#92): df above ~190 drove the underlying incomplete_gamma
        // series past its old 100-iteration cap and the infallible cdf panicked.
        // At the mean the chi-square CDF is just above 0.5 (right-skewed => mean
        // above median). Reference value 0.505947 (df = 1000) cross-checked with
        // the Wilson-Hilferty approximation (0.5059).
        let d1000 = CumulativeChiSquareDistribution::new(1000.0).unwrap();
        assert!((d1000.cdf(1000.0) - 0.5059471460854907).abs() < 1e-12);
        assert!((d1000.cdf(1200.0) - 0.9999877440576714).abs() < 1e-12);

        // Sweep the mean region for several large df: in range and monotone.
        for df in [400.0, 1000.0, 5000.0] {
            let dist = CumulativeChiSquareDistribution::new(df).unwrap();
            let mut prev = 0.0;
            let mut x = 0.5 * df;
            while x <= 1.5 * df {
                let p = dist.cdf(x);
                assert!((0.0..=1.0).contains(&p), "cdf({x}) = {p} for df={df}");
                assert!(p >= prev, "not increasing at x={x} for df={df}");
                prev = p;
                x += 0.02 * df;
            }
        }
    }

    #[test]
    fn nan_x_is_nan() {
        assert!(
            CumulativeChiSquareDistribution::new(3.0)
                .unwrap()
                .cdf(Real::NAN)
                .is_nan()
        );
    }

    #[test]
    fn new_rejects_nonpositive_nan_and_infinite_df() {
        assert!(CumulativeChiSquareDistribution::new(0.0).is_err());
        assert!(CumulativeChiSquareDistribution::new(-1.0).is_err());
        assert!(CumulativeChiSquareDistribution::new(Real::NAN).is_err());
        assert!(CumulativeChiSquareDistribution::new(Real::INFINITY).is_err());
    }
}
