//! Poisson distribution.
//!
//! Port of `ql/math/distributions/poissondistribution.hpp`: the Poisson mass
//! function [`PoissonDistribution`], its CDF [`CumulativePoissonDistribution`],
//! and the inverse [`InverseCumulativePoisson`].
//!
//! Being a discrete distribution, the mass and CDF take an integer count
//! (`u64`, matching QuantLib's `BigNatural`) rather than the `Real`-domain
//! [`Density`](super::Density) / [`Cdf`](super::Cdf) traits. The inverse maps a
//! probability to a count and so does implement the [`Quantile`] trait.
//!
//! The CDF and inverse accumulate the mass via the recurrence
//! `P(j) = P(j-1)·mu/j` (from `P(0) = e^{-mu}`) rather than QuantLib's
//! `1 - incompleteGammaFunction(k+1, mu)`. This is a deliberate numerical
//! deviation: it avoids the Lanczos `log_gamma` error inside the incomplete
//! gamma, matches the reference recurrence the test-suite uses exactly, and
//! keeps the mass/CDF/inverse internally consistent. The recurrence seed
//! `e^{-mu}` underflows for `mu` beyond ~708, so [`CumulativePoissonDistribution`]
//! and [`InverseCumulativePoisson`] reject means above that (normal-approximation
//! territory); [`PoissonDistribution::pmf`] is log-space and keeps the wider domain.

use super::{Probability, Quantile};
use crate::errors::QlResult;
use crate::types::Real;
use crate::{fail, require};

/// The Poisson probability mass function with mean `mu >= 0`.
#[derive(Clone, Copy, Debug)]
pub struct PoissonDistribution {
    mu: Real,
    log_mu: Real,
}

impl PoissonDistribution {
    /// A Poisson mass function with mean `mu`.
    ///
    /// # Errors
    ///
    /// Returns an error unless `mu` is finite and `>= 0`.
    pub fn new(mu: Real) -> QlResult<Self> {
        require!(
            mu.is_finite() && mu >= 0.0,
            "Poisson mean must be a finite non-negative number, got {mu}"
        );
        let log_mu = if mu == 0.0 { 0.0 } else { mu.ln() };
        Ok(PoissonDistribution { mu, log_mu })
    }

    /// The probability mass `P(X = k)`.
    pub fn pmf(&self, k: u64) -> Real {
        if self.mu == 0.0 {
            return if k == 0 { 1.0 } else { 0.0 };
        }
        // exp(k·ln(mu) - ln(k!) - mu). ln(k!) is summed directly as Σ ln(j):
        // the Lanczos log_gamma's ~1e-10 relative error, scaled by ln(k!),
        // would exceed the mass tolerance, whereas the sum is accurate to ~1e-15
        // (QuantLib uses an exact factorial table here for the same reason).
        let ln_factorial: Real = (1..=k).map(|j| (j as Real).ln()).sum();
        (k as Real * self.log_mu - ln_factorial - self.mu).exp()
    }
}

/// The cumulative distribution function of the Poisson distribution.
#[derive(Clone, Copy, Debug)]
pub struct CumulativePoissonDistribution {
    mu: Real,
}

impl CumulativePoissonDistribution {
    /// A Poisson CDF with mean `mu`.
    ///
    /// # Errors
    ///
    /// Returns an error unless `mu` is finite, `>= 0`, and small enough that the
    /// mass recurrence's seed `e^{-mu}` stays a normal float (roughly
    /// `mu <= 708`); beyond that it underflows and the CDF would collapse to 0.
    /// Such means are normal-approximation territory.
    pub fn new(mu: Real) -> QlResult<Self> {
        require!(
            mu.is_finite() && mu >= 0.0,
            "Poisson mean must be a finite non-negative number, got {mu}"
        );
        if (-mu).exp() < Real::MIN_POSITIVE {
            fail!("Poisson mean too large for the mass recurrence (e^-mu underflows), got {mu}");
        }
        Ok(CumulativePoissonDistribution { mu })
    }

    /// `P(X <= k) = Σ_{j=0}^{k} e^{-mu} mu^j / j!`.
    pub fn cdf(&self, k: u64) -> Real {
        // Accumulate the mass via the recurrence P(j) = P(j-1)·mu/j from
        // P(0) = e^{-mu}. mu = 0 falls out: P(0) = 1 and every later term is 0.
        let mut term = (-self.mu).exp();
        let mut sum = term;
        for j in 1..=k {
            term *= self.mu / j as Real;
            sum += term;
        }
        sum
    }
}

/// The inverse cumulative Poisson distribution with rate `lambda > 0`.
#[derive(Clone, Copy, Debug)]
pub struct InverseCumulativePoisson {
    lambda: Real,
}

impl InverseCumulativePoisson {
    /// An inverse cumulative Poisson with rate `lambda`.
    ///
    /// # Errors
    ///
    /// Returns an error unless `lambda` is finite, `> 0`, and small enough that
    /// the mass-accumulation seed `e^{-lambda}` stays a normal float (roughly
    /// `lambda <= 708`). Beyond that the seed underflows to 0 and the
    /// accumulation loop would never reach `p` - a hang, so it is rejected.
    pub fn new(lambda: Real) -> QlResult<Self> {
        require!(
            lambda.is_finite() && lambda > 0.0,
            "Poisson lambda must be a finite positive number, got {lambda}"
        );
        if (-lambda).exp() < Real::MIN_POSITIVE {
            fail!(
                "Poisson lambda too large for the mass recurrence (e^-lambda underflows), got {lambda}"
            );
        }
        Ok(InverseCumulativePoisson { lambda })
    }
}

impl Quantile for InverseCumulativePoisson {
    /// The smallest count `k` whose CDF reaches `p`, returned as a `Real`.
    ///
    /// `quantile(0)` is `0` and `quantile(1)` is `+∞` (QuantLib's `QL_MAX_REAL`).
    ///
    /// # Errors
    ///
    /// Returns an error when `p` is so close to 1 that the mass accumulation
    /// plateaus below it before reaching `p` - the upper tail is unresolvable at
    /// this floating-point precision.
    fn quantile(&self, p: Probability) -> QlResult<Real> {
        let p = p.value();
        if p == 0.0 {
            return Ok(0.0);
        }
        if p == 1.0 {
            return Ok(Real::INFINITY);
        }
        // Accumulate the mass P(X = index) until the running CDF reaches p,
        // using the recurrence P(k) = P(k-1)·lambda/k from P(0) = e^{-lambda}.
        let mut sum = 0.0;
        let mut index: u64 = 0;
        let mut mass = (-self.lambda).exp();
        while p > sum {
            // Once a tail mass falls below the running sum's ULP, the sum can no
            // longer advance; if it has plateaued below p, the target lies in the
            // unresolvable upper tail (p within rounding of 1). Fail rather than
            // loop forever.
            if sum + mass == sum {
                fail!(
                    "Poisson quantile cannot resolve p={p} (too close to 1) for lambda={}",
                    self.lambda
                );
            }
            sum += mass;
            index += 1;
            mass *= self.lambda / index as Real;
        }
        Ok((index - 1) as Real)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Port of QuantLib's testPoisson: across means 0, 0.5, ..., 10, the mass at
    // k=0 is e^{-mu} and each subsequent mass follows the log-recurrence.
    #[test]
    fn pmf_matches_reference() {
        let mut mean = 0.0;
        while mean <= 10.0 {
            let pdf = PoissonDistribution::new(mean).unwrap();
            let mut log_helper = -mean;
            assert!((pdf.pmf(0) - log_helper.exp()).abs() <= 1e-16);
            for i in 1..25u64 {
                let expected = if mean == 0.0 {
                    0.0
                } else {
                    log_helper = log_helper + mean.ln() - (i as Real).ln();
                    log_helper.exp()
                };
                assert!(
                    (pdf.pmf(i) - expected).abs() <= 1e-13,
                    "pmf(mean={mean})({i}) = {} vs {expected}",
                    pdf.pmf(i)
                );
            }
            mean += 0.5;
        }
    }

    // Port of QuantLib's testCumulativePoisson.
    #[test]
    fn cdf_matches_reference() {
        let mut mean = 0.0;
        while mean <= 10.0 {
            let cdf = CumulativePoissonDistribution::new(mean).unwrap();
            let mut log_helper = -mean;
            let mut cum_expected = log_helper.exp();
            assert!((cdf.cdf(0) - cum_expected).abs() <= 1e-13);
            for i in 1..25u64 {
                if mean == 0.0 {
                    cum_expected = 1.0;
                } else {
                    log_helper = log_helper + mean.ln() - (i as Real).ln();
                    cum_expected += log_helper.exp();
                }
                assert!(
                    (cdf.cdf(i) - cum_expected).abs() <= 1e-12,
                    "cdf(mean={mean})({i}) = {} vs {cum_expected}",
                    cdf.cdf(i)
                );
            }
            mean += 0.5;
        }
    }

    // Port of QuantLib's testInverseCumulativePoisson: with lambda = 1, the
    // tabulated probabilities invert to the counts 0, 1, 2, ...
    #[test]
    fn inverse_matches_reference() {
        let icp = InverseCumulativePoisson::new(1.0).unwrap();
        let data = [
            0.2, 0.5, 0.9, 0.98, 0.99, 0.999, 0.9999, 0.99995, 0.99999, 0.999999, 0.9999999,
            0.99999999,
        ];
        for (i, &x) in data.iter().enumerate() {
            let got = icp.quantile(Probability::try_from(x).unwrap()).unwrap();
            assert_eq!(got, i as Real, "icp({x}) = {got}, expected {i}");
        }
    }

    #[test]
    fn mu_zero_is_point_mass_at_zero() {
        let pdf = PoissonDistribution::new(0.0).unwrap();
        assert_eq!(pdf.pmf(0), 1.0);
        assert_eq!(pdf.pmf(1), 0.0);
        let cdf = CumulativePoissonDistribution::new(0.0).unwrap();
        assert_eq!(cdf.cdf(0), 1.0);
        assert_eq!(cdf.cdf(5), 1.0);
    }

    #[test]
    fn inverse_endpoints() {
        let icp = InverseCumulativePoisson::new(2.5).unwrap();
        assert_eq!(
            icp.quantile(Probability::try_from(0.0).unwrap()).unwrap(),
            0.0
        );
        assert_eq!(
            icp.quantile(Probability::try_from(1.0).unwrap()).unwrap(),
            Real::INFINITY
        );
    }

    // For a large (but accepted) lambda the accumulated CDF plateaus just below
    // 1; a p within rounding of 1 must error rather than spin forever.
    #[test]
    fn inverse_errors_for_p_within_rounding_of_one() {
        let icp = InverseCumulativePoisson::new(707.0).unwrap();
        let p = Probability::try_from(1.0_f64.next_down()).unwrap();
        assert!(icp.quantile(p).is_err());
    }

    #[test]
    fn constructors_reject_invalid_parameters() {
        assert!(PoissonDistribution::new(-1.0).is_err());
        assert!(PoissonDistribution::new(Real::NAN).is_err());
        assert!(PoissonDistribution::new(Real::INFINITY).is_err());
        assert!(CumulativePoissonDistribution::new(-0.5).is_err());
        // lambda must be strictly positive for the inverse.
        assert!(InverseCumulativePoisson::new(0.0).is_err());
        assert!(InverseCumulativePoisson::new(Real::INFINITY).is_err());
    }

    // Means above ~708 underflow the recurrence seed e^{-mu}: the CDF would
    // collapse to 0 and the inverse loop would hang, so they are rejected.
    #[test]
    fn recurrence_paths_reject_means_that_underflow() {
        assert!(CumulativePoissonDistribution::new(750.0).is_err());
        assert!(InverseCumulativePoisson::new(750.0).is_err());
        // The PMF is log-space, so it keeps the wider domain.
        assert!(PoissonDistribution::new(750.0).is_ok());
        // A large-but-supported mean stays well-behaved (no underflow/hang).
        let cdf = CumulativePoissonDistribution::new(700.0).unwrap();
        assert!(
            cdf.cdf(700) > 0.4 && cdf.cdf(700) < 0.6,
            "cdf(700) = {}",
            cdf.cdf(700)
        );
        let icp = InverseCumulativePoisson::new(700.0).unwrap();
        let median = icp.quantile(Probability::try_from(0.5).unwrap()).unwrap();
        assert!((median - 700.0).abs() < 25.0, "median = {median}");
    }
}
