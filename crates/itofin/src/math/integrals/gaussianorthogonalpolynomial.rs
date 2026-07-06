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
}
