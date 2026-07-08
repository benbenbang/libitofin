//! Segment (fixed-interval trapezoid) integration.
//!
//! Port of `SegmentIntegral` from
//! `ql/math/integrals/segmentintegral.{hpp,cpp}`: the interval is split into a
//! fixed number of equal segments and integrated with the composite trapezoid
//! rule. Non-adaptive, so it carries no accuracy or evaluation budget.

use crate::errors::QlResult;
use crate::fail;
use crate::math::comparison::close_enough;
use crate::math::integrals::Integrator;
use crate::types::{Real, Size};

/// Composite-trapezoid integrator over a fixed number of equal intervals.
pub struct SegmentIntegral {
    intervals: Size,
}

impl SegmentIntegral {
    /// A segment integrator using `intervals` equal sub-intervals (at least 1).
    pub fn new(intervals: Size) -> QlResult<Self> {
        if intervals == 0 {
            fail!("at least 1 interval needed, 0 given");
        }
        Ok(SegmentIntegral { intervals })
    }
}

impl Integrator for SegmentIntegral {
    fn integrate_impl<F>(&self, f: &mut F, a: Real, b: Real) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        if close_enough(a, b) {
            return Ok(0.0);
        }
        // Composite trapezoid: half-weight the endpoints, full-weight the
        // interior nodes. The `x < end` bound stops just short of b so rounding
        // never adds an extra node past it.
        let dx = (b - a) / self.intervals as Real;
        let mut sum = 0.5 * (f(a) + f(b));
        let end = b - 0.5 * dx;
        let mut x = a + dx;
        while x < end {
            sum += f(x);
            x += dx;
        }
        Ok(sum * dx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::distributions::normal::NormalDistribution;

    const TOL: Real = 1e-6;

    fn integral(f: impl FnMut(Real) -> Real, a: Real, b: Real) -> Real {
        SegmentIntegral::new(10_000)
            .unwrap()
            .integrate(f, a, b)
            .unwrap()
    }

    #[test]
    fn reproduces_known_integrals() {
        // Ported from QuantLib's testSeveral (Abcd case omitted, not yet ported).
        assert!((integral(|_| 0.0, 0.0, 1.0) - 0.0).abs() < TOL);
        assert!((integral(|_| 1.0, 0.0, 1.0) - 1.0).abs() < TOL);
        assert!((integral(|x| x, 0.0, 1.0) - 0.5).abs() < TOL);
        assert!((integral(|x| x * x, 0.0, 1.0) - 1.0 / 3.0).abs() < TOL);
        assert!((integral(|x| x.sin(), 0.0, std::f64::consts::PI) - 2.0).abs() < TOL);
        assert!((integral(|x| x.cos(), 0.0, std::f64::consts::PI) - 0.0).abs() < TOL);
        // The standard normal density integrates to 1 over [-10, 10].
        let g = NormalDistribution::standard();
        assert!((integral(|x| g.value(x), -10.0, 10.0) - 1.0).abs() < TOL);
    }

    #[test]
    fn degenerate_and_reversed_limits() {
        let seg = SegmentIntegral::new(100).unwrap();
        // a == b integrates to zero.
        assert_eq!(seg.integrate(|x| x, 2.0, 2.0).unwrap(), 0.0);
        // A tiny [1, 1 + eps] domain of the zero function (QuantLib's
        // testDegeneratedDomain).
        assert_eq!(
            seg.integrate(|_| 0.0, 1.0, 1.0 + Real::EPSILON).unwrap(),
            0.0
        );
        // Reversed limits negate the result: int_1^0 x dx = -0.5.
        assert!((seg.integrate(|x| x, 1.0, 0.0).unwrap() - (-0.5)).abs() < TOL);
    }

    #[test]
    fn zero_intervals_rejected() {
        assert!(SegmentIntegral::new(0).is_err());
    }

    #[test]
    fn non_finite_bounds_rejected() {
        // The finite-interval driver rejects NaN/infinite bounds instead of
        // routing them into a silent Ok(NaN).
        let seg = SegmentIntegral::new(100).unwrap();
        for &(a, b) in &[
            (Real::NAN, 1.0),
            (0.0, Real::NAN),
            (Real::NEG_INFINITY, 0.0),
            (0.0, Real::INFINITY),
        ] {
            assert!(seg.integrate(|x| x, a, b).is_err(), "bounds [{a}, {b}]");
        }
    }
}
