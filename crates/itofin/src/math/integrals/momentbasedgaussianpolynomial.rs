//! Gaussian quadrature defined by the moments of the distribution.
//!
//! Port of `ql/math/integrals/momentbasedgaussianpolynomial.hpp`: derives the
//! three-term recurrence coefficients `alpha_k`, `beta_k` of an orthogonal
//! polynomial family from the raw moments of its weight function, via the
//! classical moment-determinant recursion. References: Golub and Welsch,
//! "Calculation of Gauss quadrature rules"; Morandi Cecchi and Redivo Zaglia,
//! "Computing the coefficients of a recurrence formula for numerical
//! integration by moments and modified moments".
//!
//! QuantLib templates the class on a multiprecision scalar `mp_real`; only the
//! `Real` (double) instantiation is exercised by its test suite and only that
//! one is ported here.

use std::cell::RefCell;

use crate::math::comparison::close_enough;
use crate::types::{Real, Size};

use super::gaussianorthogonalpolynomial::GaussianOrthogonalPolynomial;

/// Grow-on-demand memoization over a `RefCell<Vec<Real>>` cache with `NaN`
/// marking the not-yet-computed entries, shared by the recurrence caches
/// here and in the Laguerre-trigonometric families. The borrow is released
/// before `compute` runs, so the closure may recurse into functions backed
/// by the same cache.
pub(super) fn memoized(
    cache: &RefCell<Vec<Real>>,
    n: Size,
    compute: impl FnOnce() -> Real,
) -> Real {
    {
        let mut c = cache.borrow_mut();
        if c.len() <= n {
            c.resize(n + 1, Real::NAN);
        }
        if !c[n].is_nan() {
            return c[n];
        }
    }
    let value = compute();
    cache.borrow_mut()[n] = value;
    value
}

/// A polynomial family described by the raw moments of its weight function,
/// `moment(i) = integral of x^i w(x) dx`, together with the weight itself.
///
/// Port of the pure virtuals a `MomentBasedGaussianPolynomial` subclass
/// supplies in QuantLib; the recurrence-coefficient machinery lives in
/// [`MomentBasedGaussianPolynomial`], which wraps an implementor of this
/// trait (composition replacing the C++ inheritance).
pub trait MomentBasedPolynomial {
    /// The raw moment `integral of x^i w(x) dx` of the weight function.
    fn moment(&self, i: Size) -> Real;

    /// The weight function `w(x)`.
    fn w(&self, x: Real) -> Real;
}

/// Adapter deriving the Gaussian-quadrature recurrence coefficients from the
/// moments of `P`, with the same memoized triangular-array recursion as
/// QuantLib (caches grow on demand, `NaN` marking the not-yet-computed
/// entries).
///
/// The moment-to-coefficient map is notoriously ill-conditioned (Gautschi,
/// "How and how not to check Gaussian quadrature formulae"), which is why the
/// C++ original offers multiprecision instantiations; in double precision it
/// is usable only for moderate orders, matching QuantLib's `Real`
/// instantiation.
pub struct MomentBasedGaussianPolynomial<P: MomentBasedPolynomial> {
    poly: P,
    b: RefCell<Vec<Real>>,
    c: RefCell<Vec<Real>>,
    z: RefCell<Vec<Vec<Real>>>,
}

impl<P: MomentBasedPolynomial> MomentBasedGaussianPolynomial<P> {
    /// Wraps the moment provider `poly`.
    pub fn new(poly: P) -> Self {
        MomentBasedGaussianPolynomial {
            poly,
            b: RefCell::new(Vec::new()),
            c: RefCell::new(Vec::new()),
            z: RefCell::new(vec![Vec::new()]),
        }
    }

    fn ensure_z(&self, k: Size, i: Size) {
        let mut z = self.z.borrow_mut();
        let cols = z[0].len().max(i + 1);
        for row in z.iter_mut() {
            if row.len() < cols {
                row.resize(cols, Real::NAN);
            }
        }
        if z.len() <= k {
            z.resize(k + 1, vec![Real::NAN; cols]);
        }
    }

    fn z(&self, k: Size, i: Size) -> Real {
        self.ensure_z(k, i);
        let cached = self.z.borrow()[k][i];
        if !cached.is_nan() {
            return cached;
        }
        let value = if k == 0 {
            self.poly.moment(i)
        } else {
            let mut value = self.z(k - 1, i + 1) - self.alpha_(k - 1) * self.z(k - 1, i);
            if k >= 2 {
                value -= self.beta_(k - 1) * self.z(k - 2, i);
            }
            value
        };
        self.z.borrow_mut()[k][i] = value;
        value
    }

    fn alpha_(&self, u: Size) -> Real {
        memoized(&self.b, u, || {
            if u == 0 {
                self.poly.moment(1)
            } else {
                -self.z(u - 1, u) / self.z(u - 1, u - 1) + self.z(u, u + 1) / self.z(u, u)
            }
        })
    }

    fn beta_(&self, u: Size) -> Real {
        if u == 0 {
            return 1.0;
        }
        memoized(&self.c, u, || self.z(u, u) / self.z(u - 1, u - 1))
    }
}

impl<P: MomentBasedPolynomial> GaussianOrthogonalPolynomial for MomentBasedGaussianPolynomial<P> {
    /// # Panics
    ///
    /// Panics unless the zeroth moment is one, mirroring the `QL_REQUIRE` in
    /// QuantLib's `Real` instantiation: the moment sequence must belong to a
    /// normalized weight function.
    fn mu_0(&self) -> Real {
        let m0 = self.poly.moment(0);
        assert!(close_enough(m0, 1.0), "zero moment must be one");
        m0
    }

    fn alpha(&self, i: Size) -> Real {
        self.alpha_(i)
    }

    fn beta(&self, i: Size) -> Real {
        self.beta_(i)
    }

    fn w(&self, x: Real) -> Real {
        self.poly.w(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::integrals::gaussianorthogonalpolynomial::GaussLaguerrePolynomial;

    struct MomentBasedGaussLaguerrePolynomial;

    impl MomentBasedPolynomial for MomentBasedGaussLaguerrePolynomial {
        fn moment(&self, i: Size) -> Real {
            if i == 0 {
                1.0
            } else {
                i as Real * self.moment(i - 1)
            }
        }

        fn w(&self, x: Real) -> Real {
            (-x).exp()
        }
    }

    #[test]
    fn moment_based_polynomial_reproduces_laguerre_recurrence() {
        let g = GaussLaguerrePolynomial::new(0.0).expect("0 > -1");
        let k = MomentBasedGaussianPolynomial::new(MomentBasedGaussLaguerrePolynomial);

        let tol = 1e-12;
        for i in 0..10 {
            let diff_alpha = (k.alpha(i) - g.alpha(i)).abs();
            assert!(
                diff_alpha <= tol,
                "failed to reproduce alpha for Laguerre quadrature: \
                 calculated {}, expected {}, diff {}",
                k.alpha(i),
                g.alpha(i),
                diff_alpha
            );
            if i > 0 {
                let diff_beta = (k.beta(i) - g.beta(i)).abs();
                assert!(
                    diff_beta <= tol,
                    "failed to reproduce beta for Laguerre quadrature: \
                     calculated {}, expected {}, diff {}",
                    k.beta(i),
                    g.beta(i),
                    diff_beta
                );
            }
        }
    }

    #[test]
    fn mu_0_is_the_zeroth_moment() {
        let k = MomentBasedGaussianPolynomial::new(MomentBasedGaussLaguerrePolynomial);
        assert_eq!(k.mu_0(), 1.0);
    }
}
