//! Binomial distribution.
//!
//! Port of `ql/math/distributions/binomialdistribution.{hpp,cpp}`: the binomial
//! mass function [`BinomialDistribution`] and its CDF
//! [`CumulativeBinomialDistribution`] (`P(X <= k) = 1 - I_p(k+1, n-k)` via the
//! regularized [`incomplete_beta`]).
//!
//! Discrete, so the mass and CDF take an integer count (`u64`, matching
//! QuantLib's `BigNatural`). The mass forms `ln C(n, k)` from exact factorial
//! log-sums rather than the Lanczos `log_gamma`, whose error would dominate
//! these factorial-sensitive values (the same reasoning as the Poisson mass).

use crate::errors::QlResult;
use crate::math::beta::incomplete_beta;
use crate::require;
use crate::types::Real;

/// `ln(m!)` summed directly as `Σ ln(j)` (accurate to ~1e-15, unlike the
/// Lanczos `log_gamma(m+1)`).
fn ln_factorial(m: u64) -> Real {
    (1..=m).map(|j| (j as Real).ln()).sum()
}

/// `ln C(n, k)`; requires `k <= n`.
fn ln_binomial_coefficient(n: u64, k: u64) -> Real {
    ln_factorial(n) - ln_factorial(k) - ln_factorial(n - k)
}

/// The binomial probability mass function for `n` trials with success
/// probability `p`.
#[derive(Clone, Copy, Debug)]
pub struct BinomialDistribution {
    n: u64,
    log_p: Real,
    log_one_minus_p: Real,
}

impl BinomialDistribution {
    /// A binomial mass function with success probability `p` over `n` trials.
    ///
    /// # Errors
    ///
    /// Returns an error unless `p` lies in `[0, 1]` (so `NaN` is rejected).
    pub fn new(p: Real, n: u64) -> QlResult<Self> {
        require!(
            (0.0..=1.0).contains(&p),
            "binomial probability must be in [0, 1], got {p}"
        );
        // At p = 0 or p = 1 one log is -inf, but the corresponding pmf branch
        // short-circuits before it is used.
        Ok(BinomialDistribution {
            n,
            log_p: p.ln(),
            log_one_minus_p: (1.0 - p).ln(),
        })
    }

    /// The probability mass `P(X = k)`.
    pub fn pmf(&self, k: u64) -> Real {
        if k > self.n {
            return 0.0;
        }
        if self.log_p == 0.0 {
            // p == 1: all mass at n.
            return if k == self.n { 1.0 } else { 0.0 };
        }
        if self.log_one_minus_p == 0.0 {
            // p == 0: all mass at 0.
            return if k == 0 { 1.0 } else { 0.0 };
        }
        (ln_binomial_coefficient(self.n, k)
            + k as Real * self.log_p
            + (self.n - k) as Real * self.log_one_minus_p)
            .exp()
    }
}

/// The cumulative distribution function of the binomial distribution.
#[derive(Clone, Copy, Debug)]
pub struct CumulativeBinomialDistribution {
    n: u64,
    p: Real,
}

impl CumulativeBinomialDistribution {
    /// A binomial CDF with success probability `p` over `n` trials.
    ///
    /// # Errors
    ///
    /// Returns an error unless `p` lies in `[0, 1]` (so `NaN` is rejected).
    pub fn new(p: Real, n: u64) -> QlResult<Self> {
        require!(
            (0.0..=1.0).contains(&p),
            "binomial probability must be in [0, 1], got {p}"
        );
        Ok(CumulativeBinomialDistribution { n, p })
    }

    /// `P(X <= k) = 1 - I_p(k+1, n-k)`.
    pub fn cdf(&self, k: u64) -> Real {
        if k >= self.n {
            return 1.0;
        }
        // a = k+1 >= 1, b = n-k >= 1, x = p in [0, 1]: incomplete_beta is valid;
        // its only residual failure is non-convergence, asserted away per the
        // eval error-boundary convention.
        1.0 - incomplete_beta((k + 1) as Real, (self.n - k) as Real, self.p)
            .expect("incomplete_beta converges for valid binomial CDF parameters")
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
    fn pmf_single_trial() {
        let dist = BinomialDistribution::new(0.3, 1).unwrap();
        assert_close(dist.pmf(0), 0.7, 1e-15);
        assert_close(dist.pmf(1), 0.3, 1e-15);
        assert_eq!(dist.pmf(2), 0.0);
    }

    #[test]
    fn pmf_matches_fair_coin_closed_form() {
        // Binomial(0.5, 4): pmf(k) = C(4, k) / 16.
        let dist = BinomialDistribution::new(0.5, 4).unwrap();
        let expected = [1.0, 4.0, 6.0, 4.0, 1.0].map(|c| c / 16.0);
        for (k, &e) in expected.iter().enumerate() {
            assert_close(dist.pmf(k as u64), e, 1e-13);
        }
    }

    #[test]
    fn pmf_sums_to_one() {
        for &(p, n) in &[(0.2, 10u64), (0.5, 25), (0.85, 40)] {
            let dist = BinomialDistribution::new(p, n).unwrap();
            let total: Real = (0..=n).map(|k| dist.pmf(k)).sum();
            assert_close(total, 1.0, 1e-12);
        }
    }

    #[test]
    fn pmf_is_symmetric_under_p_swap() {
        // Binomial(p, n).pmf(k) == Binomial(1-p, n).pmf(n-k).
        let a = BinomialDistribution::new(0.3, 12).unwrap();
        let b = BinomialDistribution::new(0.7, 12).unwrap();
        for k in 0..=12u64 {
            assert_close(a.pmf(k), b.pmf(12 - k), 1e-13);
        }
    }

    #[test]
    fn pmf_degenerate_p() {
        let zero = BinomialDistribution::new(0.0, 5).unwrap();
        assert_eq!(zero.pmf(0), 1.0);
        assert_eq!(zero.pmf(3), 0.0);
        let one = BinomialDistribution::new(1.0, 5).unwrap();
        assert_eq!(one.pmf(5), 1.0);
        assert_eq!(one.pmf(4), 0.0);
    }

    #[test]
    fn cdf_equals_cumulative_pmf() {
        // The CDF goes through incomplete_beta (Lanczos log_gamma inside, ~1e-12
        // accurate), while the running PMF sum uses exact log-factorials, so they
        // agree only to incomplete_beta's tolerance, not to ~1e-15.
        for &(p, n) in &[(0.2, 10u64), (0.5, 15), (0.75, 20)] {
            let pmf = BinomialDistribution::new(p, n).unwrap();
            let cdf = CumulativeBinomialDistribution::new(p, n).unwrap();
            let mut running = 0.0;
            for k in 0..=n {
                running += pmf.pmf(k);
                assert_close(cdf.cdf(k), running, 1e-11);
            }
        }
    }

    #[test]
    fn cdf_boundaries() {
        let cdf = CumulativeBinomialDistribution::new(0.4, 8).unwrap();
        assert_eq!(cdf.cdf(8), 1.0);
        assert_eq!(cdf.cdf(100), 1.0);
    }

    #[test]
    fn cdf_degenerate_p() {
        // p = 0: all mass at 0, so cdf(k) = 1 for every k >= 0.
        let zero = CumulativeBinomialDistribution::new(0.0, 6).unwrap();
        assert_eq!(zero.cdf(0), 1.0);
        assert_eq!(zero.cdf(3), 1.0);
        // p = 1: mass at n, so cdf(k) = 0 for k < n and 1 at n.
        let one = CumulativeBinomialDistribution::new(1.0, 6).unwrap();
        assert_eq!(one.cdf(5), 0.0);
        assert_eq!(one.cdf(6), 1.0);
    }

    #[test]
    fn constructors_reject_invalid_p() {
        assert!(BinomialDistribution::new(-0.1, 5).is_err());
        assert!(BinomialDistribution::new(1.1, 5).is_err());
        assert!(BinomialDistribution::new(Real::NAN, 5).is_err());
        assert!(CumulativeBinomialDistribution::new(-0.1, 5).is_err());
        assert!(CumulativeBinomialDistribution::new(Real::NAN, 5).is_err());
    }
}
