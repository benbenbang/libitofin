//! Filon integration for oscillatory integrands.
//!
//! Port of `ql/math/integrals/filonintegral.{hpp,cpp}`: integrates
//! `f(x) * cos(t x)` ([`FilonType::Cosine`]) or `f(x) * sin(t x)`
//! ([`FilonType::Sine`]) over `[a, b]` with Filon's quadrature on an even number
//! of equal intervals, which stays accurate where a plain rule would need a very
//! fine grid to resolve the oscillation.

use crate::errors::QlResult;
use crate::fail;
use crate::math::integrals::Integrator;
use crate::types::{Real, Size};

/// A trigonometric weight (`sin` or `cos`) applied to the phase `t * x`.
type Weight = fn(Real) -> Real;

/// The oscillatory weight Filon's rule integrates the sampled function against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilonType {
    /// Integrate `f(x) * sin(t x)`.
    Sine,
    /// Integrate `f(x) * cos(t x)`.
    Cosine,
}

/// Filon integrator for `f(x) * {sin,cos}(t x)` over an even interval count.
pub struct FilonIntegral {
    kind: FilonType,
    t: Real,
    n: Size,
}

impl FilonIntegral {
    /// A Filon integrator against `sin`/`cos` of frequency `t`, splitting the
    /// domain into `intervals` equal sub-intervals.
    ///
    /// # Errors
    ///
    /// Returns an error unless `intervals` is even and positive, and `t` is
    /// finite and non-zero (Filon's coefficients divide by `t * h`).
    pub fn new(kind: FilonType, t: Real, intervals: Size) -> QlResult<Self> {
        if intervals == 0 || !intervals.is_multiple_of(2) {
            fail!("number of intervals must be even and positive, got {intervals}");
        }
        if !t.is_finite() || t == 0.0 {
            fail!("frequency t must be finite and non-zero, got {t}");
        }
        Ok(FilonIntegral {
            kind,
            t,
            n: intervals / 2,
        })
    }
}

impl Integrator for FilonIntegral {
    fn integrate_impl<F>(&self, f: &mut F, a: Real, b: Real) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        let n = self.n;
        let two_n = 2 * n;
        let h = (b - a) / two_n as Real;

        // Node abscissae, accumulated as QuantLib's Array(2n+1, a, h) does
        // (repeated += h), so the rounding matches the reference values.
        let mut x = Vec::with_capacity(two_n + 1);
        let mut xi = a;
        for _ in 0..=two_n {
            x.push(xi);
            xi += h;
        }
        let v: Vec<Real> = x.iter().map(|&xi| f(xi)).collect();

        let theta = self.t * h;
        let theta2 = theta * theta;
        let theta3 = theta2 * theta;
        // theta = t*h is the true denominator base; the constructor only knows t,
        // not h. A tiny t or interval makes theta subnormal so theta2/theta3
        // underflow to zero (and an extreme interval overflows them), and the
        // coefficients below would then divide by zero into a non-finite result.
        // The numerators are bounded sin/cos, so guarding these two denominators
        // is enough. (MIN_POSITIVE, not 0: subnormals have a degraded mantissa.)
        if !theta2.is_finite()
            || theta2 < Real::MIN_POSITIVE
            || !theta3.is_finite()
            || theta3.abs() < Real::MIN_POSITIVE
        {
            fail!("Filon phase t*h = {theta} is out of range for the coefficients over [{a}, {b}]");
        }
        let alpha =
            1.0 / theta + (2.0 * theta).sin() / (2.0 * theta2) - 2.0 * theta.sin().powi(2) / theta3;
        let beta = 2.0 * ((1.0 + theta.cos().powi(2)) / theta2 - (2.0 * theta).sin() / theta3);
        let gamma = 4.0 * (theta.sin() / theta3 - theta.cos() / theta2);

        // Cosine weights f1 = sin, f2 = cos; Sine swaps them.
        let (f1, f2): (Weight, Weight) = match self.kind {
            FilonType::Cosine => (Real::sin, Real::cos),
            FilonType::Sine => (Real::cos, Real::sin),
        };

        let t = self.t;
        // Even-index sum with half-weighted endpoints, and the odd-index sum.
        let mut c_2n = v[0] * f2(t * a) - 0.5 * (v[two_n] * f2(t * b) + v[0] * f2(t * a));
        let mut c_2n_1 = 0.0;
        for i in 1..=n {
            c_2n += v[2 * i] * f2(t * x[2 * i]);
            c_2n_1 += v[2 * i - 1] * f2(t * x[2 * i - 1]);
        }

        let sign = if self.kind == FilonType::Cosine {
            1.0
        } else {
            -1.0
        };
        Ok(
            h * (alpha * (v[two_n] * f1(t * x[two_n]) - v[0] * f1(t * x[0])) * sign
                + beta * c_2n
                + gamma * c_2n_1),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI};

    // Port of testFolinIntegration: exp(-x/2) weighted by cos(100 x) over
    // [0, 2 pi], and its sine counterpart over the shifted interval, against
    // known good values for a range of interval counts.
    #[test]
    fn matches_known_filon_values() {
        let intervals = [4, 8, 16, 128, 256, 1024, 2048];
        let expected = [
            4.55229440e-5,
            4.72338540e-5,
            4.72338540e-5,
            4.78308678e-5,
            4.78404787e-5,
            4.78381120e-5,
            4.78381084e-5,
        ];
        let t = 100.0;
        let o = FRAC_PI_2 / t;
        let tol = 1e-12;

        let cosine_f = |x: Real| (-0.5 * x).exp();
        let sine_f = |x: Real| (-0.5 * (x - FRAC_PI_2 / 100.0)).exp();

        for (i, &n) in intervals.iter().enumerate() {
            let cosine = FilonIntegral::new(FilonType::Cosine, t, n)
                .unwrap()
                .integrate(cosine_f, 0.0, 2.0 * PI)
                .unwrap();
            let sine = FilonIntegral::new(FilonType::Sine, t, n)
                .unwrap()
                .integrate(sine_f, o, 2.0 * PI + o)
                .unwrap();
            assert!(
                (cosine - expected[i]).abs() < tol,
                "cosine n={n}: {cosine} vs {}",
                expected[i]
            );
            assert!(
                (sine - expected[i]).abs() < tol,
                "sine n={n}: {sine} vs {}",
                expected[i]
            );
        }
    }

    // Sanity against an analytic value: int_0^{2 pi} 1 * cos(100 x) dx = 0.
    #[test]
    fn constant_weight_matches_analytic_zero() {
        let filon = FilonIntegral::new(FilonType::Cosine, 100.0, 128).unwrap();
        let calculated = filon.integrate(|_| 1.0, 0.0, 2.0 * PI).unwrap();
        assert!(calculated.abs() < 1e-10, "got {calculated}");
    }

    // Regression: t is finite and non-zero, but theta = t*h is subnormal so its
    // square and cube underflow to zero. The coefficients would then divide by
    // zero; integrate must return an error rather than a non-finite result.
    #[test]
    fn tiny_frequency_underflowing_the_phase_errors() {
        let filon = FilonIntegral::new(FilonType::Cosine, Real::MIN_POSITIVE, 4).unwrap();
        assert!(filon.integrate(|x| x, 0.0, 1.0).is_err());
    }

    #[test]
    fn rejects_invalid_configuration() {
        // Odd or zero interval counts.
        assert!(FilonIntegral::new(FilonType::Sine, 1.0, 3).is_err());
        assert!(FilonIntegral::new(FilonType::Sine, 1.0, 0).is_err());
        // Non-finite or zero frequency.
        for t in [0.0, Real::NAN, Real::INFINITY] {
            assert!(
                FilonIntegral::new(FilonType::Cosine, t, 4).is_err(),
                "t={t}"
            );
        }
    }
}
