//! Non-central chi-squared polynomials for Gaussian quadratures.
//!
//! Port of `ql/experimental/math/gaussiannoncentralchisquaredpolynomial.{hpp,cpp}`:
//! a moment-based family whose weight is the non-central chi-squared density
//! with `nu` degrees of freedom and non-centrality `lambda`.
//!
//! QuantLib evaluates the raw moments from a machine-generated table of 28
//! closed-form polynomials in `lambda`. This port evaluates the identical
//! closed form directly,
//!
//! ```text
//! E[X^n] = sum_{j=0..n} C(n, j) lambda^j 2^(n-j) (nu/2 + j)_(n-j)
//! ```
//!
//! (with `(a)_k` the rising factorial), an all-positive-terms sum with the
//! same polynomial-in-`lambda` coefficients as the generated table, so the
//! 28-moment cap of the C++ table does not apply here. The weight replaces
//! `boost::math::pdf(non_central_chi_squared_distribution)` with the
//! Poisson-weighted central chi-squared density series evaluated in log
//! space.

use crate::errors::QlResult;
use crate::math::gammafunction::log_gamma;
use crate::require;
use crate::types::{Real, Size};

use super::momentbasedgaussianpolynomial::MomentBasedPolynomial;

/// Moment provider for the non-central chi-squared weight; wrap in
/// [`MomentBasedGaussianPolynomial`] to obtain the quadrature family
/// (QuantLib's `GaussNonCentralChiSquaredPolynomial`).
///
/// The moment-to-recurrence map is severely ill-conditioned in double
/// precision (see the module docs of `momentbasedgaussianpolynomial`), so
/// only moderate orders are usable, as in QuantLib's `Real` instantiation.
///
/// [`MomentBasedGaussianPolynomial`]: super::momentbasedgaussianpolynomial::MomentBasedGaussianPolynomial
pub struct GaussNonCentralChiSquaredPolynomial {
    nu: Real,
    lambda: Real,
}

impl GaussNonCentralChiSquaredPolynomial {
    /// The family for `nu` degrees of freedom and non-centrality `lambda`;
    /// `lambda = 0` degenerates to the central chi-squared weight, which the
    /// C++ class accepts as well.
    ///
    /// # Errors
    ///
    /// Returns an error unless `nu > 0` and `lambda >= 0`.
    pub fn new(nu: Real, lambda: Real) -> QlResult<Self> {
        require!(nu.is_finite() && nu > 0.0, "nu must be positive");
        require!(
            lambda.is_finite() && lambda >= 0.0,
            "lambda must be non-negative"
        );
        Ok(GaussNonCentralChiSquaredPolynomial { nu, lambda })
    }
}

impl MomentBasedPolynomial for GaussNonCentralChiSquaredPolynomial {
    fn moment(&self, i: Size) -> Real {
        let mut sum = 0.0;
        let mut binom = 1.0;
        for j in 0..=i {
            let mut term = binom * self.lambda.powi(j as i32) * 2f64.powi((i - j) as i32);
            for m in 0..(i - j) {
                term *= 0.5 * self.nu + (j + m) as Real;
            }
            sum += term;
            binom *= (i - j) as Real / (j + 1) as Real;
        }
        sum
    }

    /// # Panics
    ///
    /// Panics for `x < 0`, mirroring the `domain_error` thrown by the
    /// `boost::math` density QuantLib delegates to.
    fn w(&self, x: Real) -> Real {
        non_central_chi_squared_pdf(self.nu, self.lambda, x)
    }
}

/// The non-central chi-squared density as the Poisson-weighted series of
/// central chi-squared densities, each term evaluated in log space:
/// `sum_k Poisson(k; lambda/2) chi2_pdf(x; nu + 2k)`. The Poisson bulk peaks
/// near `k = lambda/2`, so the iteration bound scales with `lambda` and the
/// two log-gamma factors are accumulated incrementally across terms.
///
/// # Panics
///
/// Panics for `x < 0`, where the density is undefined; `boost::math::pdf`,
/// which QuantLib delegates to, throws a `domain_error` there.
fn non_central_chi_squared_pdf(nu: Real, lambda: Real, x: Real) -> Real {
    assert!(
        x >= 0.0,
        "non-central chi-squared density is undefined for x < 0"
    );
    let half_lambda = 0.5 * lambda;
    if x == 0.0 {
        return match nu.partial_cmp(&2.0).expect("nu is finite") {
            std::cmp::Ordering::Less => Real::INFINITY,
            std::cmp::Ordering::Equal => 0.5 * (-half_lambda).exp(),
            std::cmp::Ordering::Greater => 0.0,
        };
    }
    let ln_x = x.ln();
    let ln_2 = std::f64::consts::LN_2;
    let ln_chi2 = |half_df: Real, ln_gamma_half_df: Real| {
        (half_df - 1.0) * ln_x - 0.5 * x - half_df * ln_2 - ln_gamma_half_df
    };
    let ln_gamma_half_nu = log_gamma(0.5 * nu).expect("nu/2 > 0");

    if half_lambda == 0.0 {
        return ln_chi2(0.5 * nu, ln_gamma_half_nu).exp();
    }
    let ln_half_lambda = half_lambda.ln();
    let k_max = 1000.max((half_lambda + 10.0 * half_lambda.sqrt()) as Size + 10);

    let mut ln_k_fact = 0.0;
    let mut ln_gamma_half_df = ln_gamma_half_nu;
    let mut sum = 0.0;
    for k in 0..=k_max {
        let k_ = k as Real;
        let half_df = 0.5 * nu + k_;
        let ln_poisson = -half_lambda + k_ * ln_half_lambda - ln_k_fact;
        let term = (ln_poisson + ln_chi2(half_df, ln_gamma_half_df)).exp();
        sum += term;
        if k_ > half_lambda && term < sum * Real::EPSILON {
            break;
        }
        ln_k_fact += (k_ + 1.0).ln();
        ln_gamma_half_df += half_df.ln();
    }
    sum
}

#[cfg(test)]
// The oracle table keeps QuantLib's multiprecision-computed digits verbatim.
#[allow(clippy::excessive_precision)]
mod tests {
    use super::*;
    use crate::math::integrals::gaussianorthogonalpolynomial::GaussianOrthogonalPolynomial;
    use crate::math::integrals::gaussianquadratures::GaussianQuadrature;
    use crate::math::integrals::momentbasedgaussianpolynomial::MomentBasedGaussianPolynomial;

    #[test]
    fn non_central_chi_squared_quadrature() {
        let poly41 = MomentBasedGaussianPolynomial::new(
            GaussNonCentralChiSquaredPolynomial::new(4.0, 1.0).expect("valid parameters"),
        );
        let quad = GaussianQuadrature::new(2, &poly41).expect("2 > 0");
        let calculated = quad.integrate(|x| x * x * poly41.w(x));
        assert!(
            (calculated - 37.0).abs() <= 1.0e-4,
            "integrating f(x) = x^2 * nonCentralChiSquared(4, 1)(x): \
             calculated {calculated}, expected 37.0"
        );

        let poly11 = MomentBasedGaussianPolynomial::new(
            GaussNonCentralChiSquaredPolynomial::new(1.0, 1.0).expect("valid parameters"),
        );
        let quad = GaussianQuadrature::new(14, &poly11).expect("14 > 0");
        let calculated = quad.integrate(|x| x * (0.1 * x).sin() * (0.3 * x).exp() * poly11.w(x));
        assert!(
            (calculated - 17.408092).abs() <= 1.0e-4,
            "integrating f(x) = x * sin(0.1*x)*exp(0.3*x)*nonCentralChiSquared(1, 1)(x): \
             calculated {calculated}, expected 17.408092"
        );
    }

    #[test]
    fn density_converges_for_large_non_centrality() {
        // Regression: a fixed 1000-term series cap truncated the Poisson
        // bulk (peaked near k = lambda/2) for lambda >= ~2000. At the mean
        // nu + lambda the density approaches the normal approximation
        // 1/sqrt(2 pi sigma^2), sigma^2 = 2(nu + 2 lambda); the Edgeworth
        // correction vanishes at the mean, so 1% is a safe tolerance.
        let (nu, lambda) = (4.0, 10000.0);
        let calculated = non_central_chi_squared_pdf(nu, lambda, nu + lambda);
        let sigma2 = 2.0 * (nu + 2.0 * lambda);
        let approx = 1.0 / (std::f64::consts::TAU * sigma2).sqrt();
        assert!(
            (calculated / approx - 1.0).abs() <= 0.01,
            "density at the mean for lambda = {lambda}: \
             calculated {calculated}, normal approximation {approx}"
        );
    }

    #[test]
    fn central_case_matches_chi_squared_density() {
        // Regression: lambda = 0 (valid in the C++ class) was rejected by
        // the constructor and NaN in the series. chi2 pdf with nu = 4 is
        // x exp(-x/2) / 4.
        let poly = GaussNonCentralChiSquaredPolynomial::new(4.0, 0.0).expect("lambda = 0 is valid");
        for x in [0.5f64, 1.0, 3.0, 10.0] {
            let expected = x * (-0.5 * x).exp() / 4.0;
            let calculated = poly.w(x);
            assert!(
                (calculated - expected).abs() <= 1e-14,
                "central chi-squared density at {x}: \
                 calculated {calculated}, expected {expected}"
            );
        }
        assert_eq!(poly.moment(1), 4.0);
    }

    #[test]
    fn density_at_the_origin_takes_the_exact_limit() {
        assert_eq!(non_central_chi_squared_pdf(4.0, 1.0, 0.0), 0.0);
        let expected = 0.5 * (-0.5f64).exp();
        assert!((non_central_chi_squared_pdf(2.0, 1.0, 0.0) - expected).abs() <= 1e-16);
        assert_eq!(non_central_chi_squared_pdf(1.0, 1.0, 0.0), Real::INFINITY);
    }

    #[test]
    #[should_panic(expected = "undefined for x < 0")]
    fn density_panics_for_negative_argument() {
        // Regression: x < 0 silently returned 0.0, which the quadrature
        // driver divides by; boost::math throws a domain_error instead.
        non_central_chi_squared_pdf(4.0, 1.0, -1.0);
    }

    #[test]
    fn non_central_chi_squared_sum_of_nodes() {
        // Walter Gautschi, How and How not to check Gaussian Quadrature
        // Formulae, test #4; expected sums computed with a multiprecision
        // library (values from QuantLib's test suite).
        let expected = [
            47.53491786730293,
            70.6103295419633383,
            98.0593406849441607,
            129.853401537905341,
            165.96963582663912,
            206.389183233992043,
        ];

        let orth_poly = MomentBasedGaussianPolynomial::new(
            GaussNonCentralChiSquaredPolynomial::new(4.0, 1.0).expect("valid parameters"),
        );

        let tol = 1e-5;
        for n in 4..10 {
            let quad = GaussianQuadrature::new(n, &orth_poly).expect("n > 0");
            let calculated: Real = quad.abscissas().iter().sum();
            assert!(
                (calculated - expected[n - 4]).abs() <= tol,
                "failed to reproduce rule of sum for n = {n}: \
                 calculated {calculated}, expected {}",
                expected[n - 4]
            );
        }
    }
}
