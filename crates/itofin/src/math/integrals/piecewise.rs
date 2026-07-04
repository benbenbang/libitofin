//! Piecewise integration around critical points.
//!
//! Port of `ql/experimental/math/piecewiseintegral.{hpp,cpp}`:
//! [`PiecewiseIntegral`] wraps another [`Integrator`] and integrates each
//! sub-interval between consecutive critical points separately, nudging the
//! sub-interval endpoints slightly away from the critical points so the inner
//! rule never samples the integrand exactly at a kink or jump.

use std::cmp::Ordering;

use crate::errors::QlResult;
use crate::math::comparison::close_enough;
use crate::math::integrals::Integrator;
use crate::types::Real;

/// Integrates piecewise between critical points, delegating each piece to an
/// inner integrator.
///
/// The inner integrator is generic (the [`Integrator`] trait is not
/// object-safe). `PiecewiseIntegral` is itself an [`Integrator`], so it composes
/// like any other rule.
pub struct PiecewiseIntegral<I> {
    integrator: I,
    critical_points: Vec<Real>,
    eps: Real,
}

impl<I: Integrator> PiecewiseIntegral<I> {
    /// A piecewise integrator over `integrator`, splitting at `critical_points`.
    ///
    /// The points are sorted and de-duplicated (by [`close_enough`]). When
    /// `avoid_critical_points` is `true`, sub-interval endpoints are nudged off
    /// each critical point by one machine epsilon so the integrand is never
    /// sampled exactly there; `false` disables the nudge.
    pub fn new(integrator: I, critical_points: Vec<Real>, avoid_critical_points: bool) -> Self {
        let mut critical_points = critical_points;
        critical_points.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        critical_points.dedup_by(|a, b| close_enough(*a, *b));
        let eps = if avoid_critical_points {
            1.0 + Real::EPSILON
        } else {
            1.0
        };
        PiecewiseIntegral {
            integrator,
            critical_points,
            eps,
        }
    }

    /// Integrates one piece, skipping intervals that collapse to a point.
    fn integrate_piece<F>(&self, f: &mut F, a: Real, b: Real) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        if close_enough(a, b) {
            Ok(0.0)
        } else {
            self.integrator.integrate(&mut *f, a, b)
        }
    }
}

impl<I: Integrator> Integrator for PiecewiseIntegral<I> {
    fn integrate_impl<F>(&self, f: &mut F, a: Real, b: Real) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        let points = &self.critical_points;
        // lower_bound: first critical point not less than the interval endpoint.
        let a0 = points.partition_point(|&p| p < a);
        let mut b0 = points.partition_point(|&p| p < b);

        // Whole interval sits beyond the last critical point: one piece, nudged
        // off the final point only if the lower limit coincides with it.
        if a0 == points.len() {
            let mut tmp = 1.0;
            if let Some(&last) = points.last()
                && close_enough(a, last)
            {
                tmp = self.eps;
            }
            return self.integrate_piece(f, a * tmp, b);
        }

        let mut res = 0.0;
        // Leading piece from the lower limit up to just below the first point.
        if !close_enough(a, points[a0]) {
            res += self.integrate_piece(f, a, (points[a0] / self.eps).min(b))?;
        }
        // Trailing piece from just above the last enclosed point to the upper
        // limit, when the upper limit is past every critical point.
        if b0 == points.len() {
            b0 -= 1;
            if !close_enough(points[b0], b) {
                res += self.integrate_piece(f, points[b0] * self.eps, b)?;
            }
        }
        // Interior pieces between consecutive critical points, each nudged in on
        // both ends and clipped to the upper limit.
        for i in a0..b0 {
            res +=
                self.integrate_piece(f, points[i] * self.eps, (points[i + 1] / self.eps).min(b))?;
        }
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::comparison::close;
    use crate::math::integrals::segment::SegmentIntegral;

    // The step function from QuantLib's testPiecewiseIntegral: breakpoints
    // x = {1..5}, levels y = {1..6}. y[min(upper_bound(x, t) - x.begin(), 5)].
    fn step(t: Real) -> Real {
        let x = [1.0, 2.0, 3.0, 4.0, 5.0];
        let y = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let idx = x.partition_point(|&xi| xi <= t);
        y[idx.min(y.len() - 1)]
    }

    // Port of testPiecewiseIntegral: a 1-segment inner rule is exact on each
    // constant piece, so the piecewise integral of the step function matches the
    // analytic area over every interval, including ones that start or end past
    // the breakpoints.
    #[test]
    fn matches_step_function_areas() {
        let piecewise = PiecewiseIntegral::new(
            SegmentIntegral::new(1).unwrap(),
            vec![1.0, 2.0, 3.0, 4.0, 5.0],
            true,
        );
        let cases = [
            (-1.0, 0.0, 1.0),
            (0.0, 1.0, 1.0),
            (0.0, 1.5, 2.0),
            (0.0, 2.0, 3.0),
            (0.0, 2.5, 4.5),
            (0.0, 3.0, 6.0),
            (0.0, 4.0, 10.0),
            (0.0, 5.0, 15.0),
            (0.0, 6.0, 21.0),
            (0.0, 7.0, 27.0),
            (3.5, 4.5, 4.5),
            (5.0, 10.0, 30.0),
            (9.0, 10.0, 6.0),
        ];
        for (a, b, expected) in cases {
            let calculated = piecewise.integrate(step, a, b).unwrap();
            assert!(
                close(calculated, expected),
                "[{a}, {b}]: calculated {calculated}, expected {expected}"
            );
        }
    }

    // The shared driver still governs the outer interval: reversed limits negate
    // and a degenerate interval integrates to zero.
    #[test]
    fn reversed_and_degenerate_outer_limits() {
        let piecewise = PiecewiseIntegral::new(
            SegmentIntegral::new(1).unwrap(),
            vec![1.0, 2.0, 3.0, 4.0, 5.0],
            true,
        );
        assert_eq!(piecewise.integrate(step, 2.0, 2.0).unwrap(), 0.0);
        // int_2.5^0 = -int_0^2.5 = -4.5.
        assert!(close(piecewise.integrate(step, 2.5, 0.0).unwrap(), -4.5));
    }

    // With no critical points every call is a single delegated integration.
    #[test]
    fn no_critical_points_delegates_whole_interval() {
        let piecewise = PiecewiseIntegral::new(SegmentIntegral::new(1000).unwrap(), vec![], true);
        assert!(close(piecewise.integrate(|x| x, 0.0, 2.0).unwrap(), 2.0));
    }
}
