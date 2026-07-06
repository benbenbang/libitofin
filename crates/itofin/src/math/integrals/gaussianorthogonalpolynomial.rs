//! Orthogonal polynomials for Gaussian quadratures.
//!
//! Port of `ql/math/integrals/gaussianorthogonalpolynomial.{hpp,cpp}`. A
//! family of orthogonal polynomials is defined by the three-term recurrence
//!
//! ```text
//! P_{k+1}(x) = (x - alpha_k) P_k(x) - beta_k P_{k-1}(x)
//! ```
//!
//! together with the zeroth moment `mu_0 = integral of w(x) dx` of its weight
//! function `w`. References: Golub and Welsch, "Calculation of Gauss
//! quadrature rules", Math. Comput. 23 (1969); "Numerical Recipes in C", 2nd
//! edition.

use crate::errors::QlResult;
use crate::math::comparison::close_enough;
use crate::math::gammafunction::log_gamma;
use crate::require;
use crate::types::{Real, Size};

/// A family of orthogonal polynomials defining a Gaussian quadrature: the
/// recurrence coefficients `alpha_k`, `beta_k`, the weight function `w`, and
/// its zeroth moment `mu_0`.
///
/// Port of the `GaussianOrthogonalPolynomial` base class; the C++ virtuals
/// are the required methods here, with the concrete `value`/`weightedValue`
/// provided on top of them.
pub trait GaussianOrthogonalPolynomial {
    /// The zeroth moment of the weight function, `integral of w(x) dx`.
    fn mu_0(&self) -> Real;

    /// The recurrence coefficient `alpha_i`.
    fn alpha(&self, i: Size) -> Real;

    /// The recurrence coefficient `beta_i`.
    fn beta(&self, i: Size) -> Real;

    /// The weight function `w(x)`.
    fn w(&self, x: Real) -> Real;

    /// The value of the monic polynomial `P_n(x)` via the three-term
    /// recurrence (iterative form of QuantLib's recursive `value`).
    fn value(&self, n: Size, x: Real) -> Real {
        let mut previous = 1.0;
        if n == 0 {
            return previous;
        }
        let mut current = x - self.alpha(0);
        for k in 1..n {
            let next = (x - self.alpha(k)) * current - self.beta(k) * previous;
            previous = current;
            current = next;
        }
        current
    }

    /// `sqrt(w(x)) * P_n(x)`.
    fn weighted_value(&self, n: Size, x: Real) -> Real {
        self.w(x).sqrt() * self.value(n, x)
    }
}

/// Gauss-Jacobi polynomial, weight `w(x) = (1-x)^alpha (1+x)^beta` on
/// `[-1, 1]`.
///
/// The Legendre, Chebyshev (both kinds) and Gegenbauer families are the
/// special cases exposed as constructors, replacing QuantLib's subclasses.
#[derive(Clone, Copy, Debug)]
pub struct GaussJacobiPolynomial {
    alpha: Real,
    beta: Real,
}

impl GaussJacobiPolynomial {
    /// A Jacobi polynomial family with weight exponents `alpha` and `beta`.
    ///
    /// # Errors
    ///
    /// Returns an error unless `alpha > -1`, `beta > -1` and
    /// `alpha + beta > -2`.
    pub fn new(alpha: Real, beta: Real) -> QlResult<Self> {
        require!(
            (alpha + beta).is_finite() && alpha + beta > -2.0,
            "alpha+beta must be bigger than -2"
        );
        require!(
            alpha.is_finite() && alpha > -1.0,
            "alpha must be bigger than -1"
        );
        require!(
            beta.is_finite() && beta > -1.0,
            "beta  must be bigger than -1"
        );
        Ok(GaussJacobiPolynomial { alpha, beta })
    }

    /// Gauss-Legendre polynomial: Jacobi with `alpha = beta = 0`, weight
    /// `w(x) = 1`.
    pub fn legendre() -> Self {
        GaussJacobiPolynomial {
            alpha: 0.0,
            beta: 0.0,
        }
    }

    /// Gauss-Chebyshev polynomial (first kind): Jacobi with
    /// `alpha = beta = -1/2`, weight `w(x) = (1-x^2)^(-1/2)`.
    pub fn chebyshev() -> Self {
        GaussJacobiPolynomial {
            alpha: -0.5,
            beta: -0.5,
        }
    }

    /// Gauss-Chebyshev polynomial (second kind): Jacobi with
    /// `alpha = beta = 1/2`, weight `w(x) = (1-x^2)^(1/2)`.
    pub fn chebyshev2nd() -> Self {
        GaussJacobiPolynomial {
            alpha: 0.5,
            beta: 0.5,
        }
    }

    /// Gauss-Gegenbauer polynomial: Jacobi with
    /// `alpha = beta = lambda - 1/2`, weight `w(x) = (1-x^2)^(lambda-1/2)`.
    ///
    /// # Errors
    ///
    /// Returns an error unless `lambda > -1/2`.
    pub fn gegenbauer(lambda: Real) -> QlResult<Self> {
        Self::new(lambda - 0.5, lambda - 0.5)
    }
}

impl GaussianOrthogonalPolynomial for GaussJacobiPolynomial {
    fn mu_0(&self) -> Real {
        // The gamma arguments are alpha+1, beta+1 and alpha+beta+2, all
        // strictly positive by construction, so log_gamma cannot fail.
        let log = log_gamma(self.alpha + 1.0).expect("alpha + 1 > 0 by construction")
            + log_gamma(self.beta + 1.0).expect("beta + 1 > 0 by construction")
            - log_gamma(self.alpha + self.beta + 2.0)
                .expect("alpha + beta + 2 > 0 by construction");
        (2.0 as Real).powf(self.alpha + self.beta + 1.0) * log.exp()
    }

    /// # Panics
    ///
    /// Panics where QuantLib throws: when the recurrence denominator vanishes
    /// and l'Hospital's rule cannot recover it (unreachable for parameters
    /// accepted by the constructors and `i >= 1`).
    fn alpha(&self, i: Size) -> Real {
        let i = i as Real;
        let mut num = self.beta * self.beta - self.alpha * self.alpha;
        let mut denom =
            (2.0 * i + self.alpha + self.beta) * (2.0 * i + self.alpha + self.beta + 2.0);

        if close_enough(denom, 0.0) {
            assert!(
                close_enough(num, 0.0),
                "can't compute a_k for jacobi integration"
            );
            num = 2.0 * self.beta;
            denom = 2.0 * (2.0 * i + self.alpha + self.beta + 1.0);
            assert!(
                !close_enough(denom, 0.0),
                "can't compute a_k for jacobi integration"
            );
        }

        num / denom
    }

    /// # Panics
    ///
    /// Panics where QuantLib throws: when the recurrence denominator vanishes
    /// and l'Hospital's rule cannot recover it (unreachable for parameters
    /// accepted by the constructors and `i >= 1`).
    fn beta(&self, i: Size) -> Real {
        let i = i as Real;
        let s = 2.0 * i + self.alpha + self.beta;
        let mut num = 4.0 * i * (i + self.alpha) * (i + self.beta) * (i + self.alpha + self.beta);
        let mut denom = s * s * (s * s - 1.0);

        if close_enough(denom, 0.0) {
            assert!(
                close_enough(num, 0.0),
                "can't compute b_k for jacobi integration"
            );
            num = 4.0 * i * (i + self.beta) * (2.0 * i + 2.0 * self.alpha + self.beta);
            denom = 2.0 * s;
            denom *= denom - 1.0;
            assert!(
                !close_enough(denom, 0.0),
                "can't compute b_k for jacobi integration"
            );
        }

        num / denom
    }

    fn w(&self, x: Real) -> Real {
        (1.0 - x).powf(self.alpha) * (1.0 + x).powf(self.beta)
    }
}

/// Generalized Gauss-Laguerre polynomial, weight `w(x; s) = x^s exp(-x)` on
/// `[0, inf)` with `s > -1`.
#[derive(Clone, Copy, Debug)]
pub struct GaussLaguerrePolynomial {
    s: Real,
}

impl GaussLaguerrePolynomial {
    /// A Laguerre polynomial family with weight exponent `s` (QuantLib
    /// defaults `s` to 0).
    ///
    /// # Errors
    ///
    /// Returns an error unless `s > -1`.
    pub fn new(s: Real) -> QlResult<Self> {
        require!(s.is_finite() && s > -1.0, "s must be bigger than -1");
        Ok(GaussLaguerrePolynomial { s })
    }
}

impl GaussianOrthogonalPolynomial for GaussLaguerrePolynomial {
    fn mu_0(&self) -> Real {
        log_gamma(self.s + 1.0)
            .expect("s + 1 > 0 by construction")
            .exp()
    }

    fn alpha(&self, i: Size) -> Real {
        2.0 * i as Real + 1.0 + self.s
    }

    fn beta(&self, i: Size) -> Real {
        let i = i as Real;
        i * (i + self.s)
    }

    fn w(&self, x: Real) -> Real {
        x.powf(self.s) * (-x).exp()
    }
}

/// Generalized Gauss-Hermite polynomial, weight
/// `w(x; mu) = |x|^(2 mu) exp(-x^2)` on the real line with `mu > -1/2`.
#[derive(Clone, Copy, Debug)]
pub struct GaussHermitePolynomial {
    mu: Real,
}

impl GaussHermitePolynomial {
    /// A Hermite polynomial family with weight exponent `mu` (QuantLib
    /// defaults `mu` to 0).
    ///
    /// # Errors
    ///
    /// Returns an error unless `mu > -1/2`.
    pub fn new(mu: Real) -> QlResult<Self> {
        require!(mu.is_finite() && mu > -0.5, "mu must be bigger than -0.5");
        Ok(GaussHermitePolynomial { mu })
    }
}

impl GaussianOrthogonalPolynomial for GaussHermitePolynomial {
    fn mu_0(&self) -> Real {
        log_gamma(self.mu + 0.5)
            .expect("mu + 1/2 > 0 by construction")
            .exp()
    }

    fn alpha(&self, _i: Size) -> Real {
        0.0
    }

    fn beta(&self, i: Size) -> Real {
        if i.is_multiple_of(2) {
            i as Real / 2.0
        } else {
            i as Real / 2.0 + self.mu
        }
    }

    fn w(&self, x: Real) -> Real {
        x.abs().powf(2.0 * self.mu) * (-x * x).exp()
    }
}

/// Gauss hyperbolic polynomial, weight `w(x) = 1/cosh(x)` on the real line.
#[derive(Clone, Copy, Debug, Default)]
pub struct GaussHyperbolicPolynomial;

impl GaussianOrthogonalPolynomial for GaussHyperbolicPolynomial {
    fn mu_0(&self) -> Real {
        std::f64::consts::PI
    }

    fn alpha(&self, _i: Size) -> Real {
        0.0
    }

    fn beta(&self, i: Size) -> Real {
        use std::f64::consts::{FRAC_PI_2, PI};
        if i != 0 {
            FRAC_PI_2 * FRAC_PI_2 * i as Real * i as Real
        } else {
            PI
        }
    }

    fn w(&self, x: Real) -> Real {
        1.0 / x.cosh()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Monic Legendre recurrence: alpha_k = 0, beta_k = k^2 / (4k^2 - 1),
    // weight w(x) = 1 on [-1, 1], mu_0 = 2.
    struct MonicLegendre;

    impl GaussianOrthogonalPolynomial for MonicLegendre {
        fn mu_0(&self) -> Real {
            2.0
        }
        fn alpha(&self, _i: Size) -> Real {
            0.0
        }
        fn beta(&self, i: Size) -> Real {
            let k = i as Real;
            k * k / (4.0 * k * k - 1.0)
        }
        fn w(&self, _x: Real) -> Real {
            1.0
        }
    }

    #[test]
    fn recurrence_reproduces_monic_legendre_polynomials() {
        let p = MonicLegendre;
        for &x in &[-0.9, -0.3, 0.0, 0.5, 1.0] {
            let x: Real = x;
            assert!((p.value(0, x) - 1.0).abs() < 1e-15);
            assert!((p.value(1, x) - x).abs() < 1e-15);
            assert!((p.value(2, x) - (x * x - 1.0 / 3.0)).abs() < 1e-15);
            assert!((p.value(3, x) - (x * x * x - 0.6 * x)).abs() < 1e-15);
        }
    }

    #[test]
    fn weighted_value_scales_by_sqrt_of_weight() {
        let p = MonicLegendre;
        assert!((p.weighted_value(2, 0.5) - p.value(2, 0.5)).abs() < 1e-15);
    }

    #[test]
    fn jacobi_with_zero_exponents_matches_legendre_recurrence() {
        let jacobi = GaussJacobiPolynomial::legendre();
        let legendre = MonicLegendre;
        assert!((jacobi.mu_0() - 2.0).abs() < 1e-14);
        for i in 0..10 {
            assert!(jacobi.alpha(i).abs() < 1e-14);
            if i > 0 {
                assert!((jacobi.beta(i) - legendre.beta(i)).abs() < 1e-14);
            }
        }
    }

    #[test]
    fn jacobi_rejects_out_of_domain_exponents() {
        assert!(GaussJacobiPolynomial::new(-1.0, 0.0).is_err());
        assert!(GaussJacobiPolynomial::new(0.0, -1.0).is_err());
        assert!(GaussJacobiPolynomial::new(-0.999, -1.5).is_err());
        assert!(GaussJacobiPolynomial::gegenbauer(-0.5).is_err());
    }

    #[test]
    fn laguerre_and_hermite_reject_out_of_domain_exponents() {
        assert!(GaussLaguerrePolynomial::new(-1.0).is_err());
        assert!(GaussHermitePolynomial::new(-0.5).is_err());
    }
}
