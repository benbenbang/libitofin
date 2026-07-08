//! Laguerre-Cosine and Laguerre-Sine Gaussian quadratures.
//!
//! Port of `ql/math/integrals/gausslaguerrecosinepolynomial.hpp`: moment-based
//! polynomial families for integrating over `[0, inf)` with the oscillating
//! weights `w(x; u) = e^{-x} (1 + cos(u x)) / m0` and
//! `w(x; u) = e^{-x} (1 + sin(u x)) / m0`, normalized so the zeroth moment is
//! one. As in the `MomentBasedGaussianPolynomial` port, only QuantLib's `Real`
//! instantiation of the `mp_real` template is ported.

use std::cell::RefCell;

use crate::types::{Real, Size};

use super::momentbasedgaussianpolynomial::{MomentBasedPolynomial, memoized};

/// The shared moment recursion of QuantLib's `GaussLaguerreTrigonometricBase`:
/// the moments of `e^{-x} trig(u x)` satisfy
/// `m_n = (2 n m_{n-1} - n (n-1) m_{n-2}) / (1 + u^2)` with family-specific
/// seeds `m_0`, `m_1`, and the factorials are the moments of the plain
/// `e^{-x}` part.
struct TrigonometricMoments {
    u: Real,
    seed0: Real,
    seed1: Real,
    m: RefCell<Vec<Real>>,
    f: RefCell<Vec<Real>>,
}

impl TrigonometricMoments {
    fn new(u: Real, seed0: Real, seed1: Real) -> Self {
        TrigonometricMoments {
            u,
            seed0,
            seed1,
            m: RefCell::new(Vec::new()),
            f: RefCell::new(Vec::new()),
        }
    }

    fn moment_(&self, n: Size) -> Real {
        memoized(&self.m, n, || match n {
            0 => self.seed0,
            1 => self.seed1,
            _ => {
                let n_ = n as Real;
                (2.0 * n_ * self.moment_(n - 1) - n_ * (n_ - 1.0) * self.moment_(n - 2))
                    / (1.0 + self.u * self.u)
            }
        })
    }

    fn fact(&self, n: Size) -> Real {
        memoized(&self.f, n, || {
            if n == 0 {
                1.0
            } else {
                n as Real * self.fact(n - 1)
            }
        })
    }
}

/// Gauss-Laguerre-Cosine integration over `[0, inf)` with weight
/// `w(x; u) = e^{-x} (1 + cos(u x)) / m0`.
///
/// Wrap in [`MomentBasedGaussianPolynomial`] to obtain the quadrature family
/// (QuantLib's `GaussLaguerreCosinePolynomial<Real>`).
///
/// [`MomentBasedGaussianPolynomial`]: super::momentbasedgaussianpolynomial::MomentBasedGaussianPolynomial
pub struct GaussLaguerreCosinePolynomial {
    moments: TrigonometricMoments,
    m0: Real,
}

impl GaussLaguerreCosinePolynomial {
    /// The cosine family with frequency `u`.
    pub fn new(u: Real) -> Self {
        let u2 = u * u;
        let seed0 = 1.0 / (1.0 + u2);
        let seed1 = (1.0 - u2) / ((1.0 + u2) * (1.0 + u2));
        GaussLaguerreCosinePolynomial {
            moments: TrigonometricMoments::new(u, seed0, seed1),
            m0: 1.0 + 1.0 / (1.0 + u2),
        }
    }
}

impl MomentBasedPolynomial for GaussLaguerreCosinePolynomial {
    fn moment(&self, i: Size) -> Real {
        (self.moments.moment_(i) + self.moments.fact(i)) / self.m0
    }

    fn w(&self, x: Real) -> Real {
        (-x).exp() * (1.0 + (self.moments.u * x).cos()) / self.m0
    }
}

/// Gauss-Laguerre-Sine integration over `[0, inf)` with weight
/// `w(x; u) = e^{-x} (1 + sin(u x)) / m0`.
///
/// Wrap in [`MomentBasedGaussianPolynomial`] to obtain the quadrature family
/// (QuantLib's `GaussLaguerreSinePolynomial<Real>`).
///
/// [`MomentBasedGaussianPolynomial`]: super::momentbasedgaussianpolynomial::MomentBasedGaussianPolynomial
pub struct GaussLaguerreSinePolynomial {
    moments: TrigonometricMoments,
    m0: Real,
}

impl GaussLaguerreSinePolynomial {
    /// The sine family with frequency `u`.
    pub fn new(u: Real) -> Self {
        let u2 = u * u;
        let seed0 = u / (1.0 + u2);
        let seed1 = 2.0 * u / ((1.0 + u2) * (1.0 + u2));
        GaussLaguerreSinePolynomial {
            moments: TrigonometricMoments::new(u, seed0, seed1),
            m0: 1.0 + u / (1.0 + u2),
        }
    }
}

impl MomentBasedPolynomial for GaussLaguerreSinePolynomial {
    fn moment(&self, i: Size) -> Real {
        (self.moments.moment_(i) + self.moments.fact(i)) / self.m0
    }

    fn w(&self, x: Real) -> Real {
        (-x).exp() * (1.0 + (self.moments.u * x).sin()) / self.m0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::integrals::gaussianquadratures::GaussianQuadrature;
    use crate::math::integrals::momentbasedgaussianpolynomial::MomentBasedGaussianPolynomial;
    use crate::types::Real;

    fn test_single(quad: &GaussianQuadrature, tag: &str, f: fn(Real) -> Real, expected: Real) {
        let calculated = quad.integrate(f);
        assert!(
            (calculated - expected).abs() <= 1.0e-4,
            "integrating {tag}: calculated {calculated}, expected {expected}"
        );
    }

    fn inv_exp(x: Real) -> Real {
        (-x).exp()
    }

    fn x_inv_exp(x: Real) -> Real {
        x * (-x).exp()
    }

    #[test]
    fn gauss_laguerre_cosine_quadrature() {
        let poly = MomentBasedGaussianPolynomial::new(GaussLaguerreCosinePolynomial::new(0.2));
        let quad = GaussianQuadrature::new(16, &poly).expect("16 > 0");

        test_single(&quad, "f(x) = exp(-x)", inv_exp, 1.0);
        test_single(&quad, "f(x) = x*exp(-x)", x_inv_exp, 1.0);
    }

    #[test]
    fn gauss_laguerre_sine_quadrature() {
        let poly = MomentBasedGaussianPolynomial::new(GaussLaguerreSinePolynomial::new(0.2));
        let quad = GaussianQuadrature::new(16, &poly).expect("16 > 0");

        test_single(&quad, "f(x) = exp(-x)", inv_exp, 1.0);
        test_single(&quad, "f(x) = x*exp(-x)", x_inv_exp, 1.0);
    }
}
