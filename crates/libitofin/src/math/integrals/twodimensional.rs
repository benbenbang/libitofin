//! Two-dimensional integration.
//!
//! Port of `ql/math/integrals/twodimensionalintegral.hpp`: the integral of a
//! function `f(x, y)` over the rectangle `[a_x, b_x] x [a_y, b_y]` by nested
//! one-dimensional integration - the outer integrator integrates
//! `x -> (inner integrator of y -> f(x, y))`.

use crate::errors::{QlError, QlResult};
use crate::fail;
use crate::math::integrals::Integrator;
use crate::types::Real;

/// Integrator for a two-dimensional function via nested 1-D integration.
///
/// The inner and outer integrators are generic (the [`Integrator`] trait is not
/// object-safe), so they may be different rules, as in QuantLib where each is a
/// separately supplied `Integrator`.
pub struct TwoDimensionalIntegral<X, Y> {
    integrator_x: X,
    integrator_y: Y,
}

impl<X: Integrator, Y: Integrator> TwoDimensionalIntegral<X, Y> {
    /// A 2-D integrator that integrates over `x` with `integrator_x` and, for
    /// each `x`, over `y` with `integrator_y`.
    pub fn new(integrator_x: X, integrator_y: Y) -> Self {
        TwoDimensionalIntegral {
            integrator_x,
            integrator_y,
        }
    }

    /// Integrates `f` over the rectangle `[a.0, b.0] x [a.1, b.1]`.
    ///
    /// # Errors
    ///
    /// Returns an error if any of the four bounds is not finite, or the first
    /// error raised by either the inner (`y`) or the outer (`x`) integration.
    pub fn integrate<F>(&self, mut f: F, a: (Real, Real), b: (Real, Real)) -> QlResult<Real>
    where
        F: FnMut(Real, Real) -> Real,
    {
        // Reject non-finite bounds on both axes up front. Otherwise a degenerate
        // x interval (a.0 == b.0) short-circuits the outer integrator to Ok(0.0)
        // before the inner integration ever runs, silently accepting invalid y
        // bounds - unlike the 1-D driver, which rejects non-finite bounds before
        // its own degenerate-interval check.
        if !a.0.is_finite() || !b.0.is_finite() || !a.1.is_finite() || !b.1.is_finite() {
            fail!(
                "integration bounds must be finite, got x = [{}, {}], y = [{}, {}]",
                a.0,
                b.0,
                a.1,
                b.1
            );
        }
        let (a_y, b_y) = (a.1, b.1);
        // The outer integrand must yield a `Real`, but the inner integration can
        // fail. Capture the first inner error and surface it once the outer
        // integration unwinds; returning 0 for the remaining outer nodes keeps
        // the outer rule well-defined (the value is discarded on error anyway).
        let mut inner_error: Option<QlError> = None;
        let result = self.integrator_x.integrate(
            |x| {
                if inner_error.is_some() {
                    return 0.0;
                }
                match self.integrator_y.integrate(|y| f(x, y), a_y, b_y) {
                    Ok(value) => value,
                    Err(error) => {
                        inner_error = Some(error);
                        0.0
                    }
                }
            },
            a.0,
            b.0,
        );
        match inner_error {
            Some(error) => Err(error),
            None => result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::integrals::trapezoid::TrapezoidIntegral;

    const TOL: Real = 1.0e-6;

    // Port of QuantLib's testTwoDimensionalIntegration: f(x, y) = x * y over
    // [0, 1] x [0, 2] integrates to 1, using a trapezoid rule on each axis.
    #[test]
    fn integrates_product_over_a_rectangle() {
        let integral = TwoDimensionalIntegral::new(
            TrapezoidIntegral::new(TOL, 1000).unwrap(),
            TrapezoidIntegral::new(TOL, 1000).unwrap(),
        );
        let calculated = integral
            .integrate(|x, y| x * y, (0.0, 0.0), (1.0, 2.0))
            .unwrap();
        assert!((calculated - 1.0).abs() < TOL, "got {calculated}");
    }

    // A separable f(x, y) = cos(x) * sin(y) factors into the product of the two
    // one-dimensional integrals: int_0^pi cos = 0, so the whole integral is 0.
    #[test]
    fn separable_integrand_matches_the_product_of_axes() {
        let integral = TwoDimensionalIntegral::new(
            TrapezoidIntegral::new(TOL, 1000).unwrap(),
            TrapezoidIntegral::new(TOL, 1000).unwrap(),
        );
        let calculated = integral
            .integrate(
                |x, y| x.cos() * y.sin(),
                (0.0, 0.0),
                (std::f64::consts::PI, 1.0),
            )
            .unwrap();
        assert!(calculated.abs() < TOL, "got {calculated}");
    }

    // A genuine inner-integration failure (finite bounds, but the inner rule
    // cannot converge) is captured and surfaced through the nested driver.
    #[test]
    fn propagates_inner_integration_errors() {
        // Outer x integrates normally; the inner y integrator is starved of
        // iterations, so it cannot reach 1e-13 on this oscillatory integrand and
        // returns an error that must propagate out of the 2-D integrate.
        let integral = TwoDimensionalIntegral::new(
            TrapezoidIntegral::new(TOL, 1000).unwrap(),
            TrapezoidIntegral::new(1e-13, 8).unwrap(),
        );
        assert!(
            integral
                .integrate(|_x, y| (50.0 * y).sin(), (0.0, 0.0), (1.0, 1.0))
                .is_err()
        );
    }

    // Regression: a degenerate x interval must not short-circuit the outer
    // integrator to Ok(0.0) and thereby swallow an invalid y bound. All four
    // coordinates are validated up front, matching the 1-D driver.
    #[test]
    fn degenerate_x_still_rejects_non_finite_y_bounds() {
        let integral = TwoDimensionalIntegral::new(
            TrapezoidIntegral::new(TOL, 1000).unwrap(),
            TrapezoidIntegral::new(TOL, 1000).unwrap(),
        );
        // a.0 == b.0 (degenerate x) with an infinite y bound: still an error.
        assert!(
            integral
                .integrate(|x, y| x * y, (0.0, 0.0), (0.0, Real::INFINITY))
                .is_err()
        );
        // A non-finite x bound is rejected as well.
        assert!(
            integral
                .integrate(|x, y| x * y, (Real::NAN, 0.0), (1.0, 2.0))
                .is_err()
        );
    }
}
