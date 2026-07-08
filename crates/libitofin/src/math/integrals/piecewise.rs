//! Piecewise integration around critical points.
//!
//! Port of `ql/experimental/math/piecewiseintegral.{hpp,cpp}`:
//! [`PiecewiseIntegral`] wraps another [`Integrator`] and integrates each
//! sub-interval between consecutive critical points separately, nudging the
//! sub-interval endpoints just off the critical points so the inner rule never
//! samples the integrand exactly at a kink or jump.
//!
//! QuantLib nudges multiplicatively (`point * (1 +/- eps)`), which does not move
//! off `0.0` at all and moves the wrong way for negative points. This port uses
//! directional next-float steps instead, so the nudge is correct for zero and
//! negative critical points too.

use crate::errors::QlResult;
use crate::fail;
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
    avoid_critical_points: bool,
}

impl<I: Integrator> PiecewiseIntegral<I> {
    /// A piecewise integrator over `integrator`, splitting at `critical_points`.
    ///
    /// The points are sorted and de-duplicated (by [`close_enough`]). When
    /// `avoid_critical_points` is `true`, sub-interval endpoints are nudged one
    /// float off each critical point so the integrand is never sampled exactly
    /// there; `false` disables the nudge.
    ///
    /// # Errors
    ///
    /// Returns an error if any critical point is not finite.
    pub fn new(
        integrator: I,
        critical_points: Vec<Real>,
        avoid_critical_points: bool,
    ) -> QlResult<Self> {
        if let Some(bad) = critical_points.iter().find(|p| !p.is_finite()) {
            fail!("critical points must be finite, got {bad}");
        }
        let mut critical_points = critical_points;
        // All finite, so total_cmp is a proper total order here.
        critical_points.sort_by(Real::total_cmp);
        critical_points.dedup_by(|a, b| close_enough(*a, *b));
        Ok(PiecewiseIntegral {
            integrator,
            critical_points,
            avoid_critical_points,
        })
    }

    /// The largest float below `p` (the left side of a critical point), or `p`
    /// itself when nudging is disabled.
    fn below(&self, p: Real) -> Real {
        if self.avoid_critical_points {
            p.next_down()
        } else {
            p
        }
    }

    /// The smallest float above `p` (the right side of a critical point), or `p`
    /// itself when nudging is disabled.
    fn above(&self, p: Real) -> Real {
        if self.avoid_critical_points {
            p.next_up()
        } else {
            p
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
        // up off the final point only if the lower limit coincides with it.
        if a0 == points.len() {
            let lower = match points.last() {
                Some(&last) if close_enough(a, last) => self.above(a),
                _ => a,
            };
            return self.integrate_piece(f, lower, b);
        }

        let mut res = 0.0;
        // Leading piece from the lower limit up to just below the first point.
        if !close_enough(a, points[a0]) {
            res += self.integrate_piece(f, a, self.below(points[a0]).min(b))?;
        }
        // Trailing piece from just above the last enclosed point to the upper
        // limit, when the upper limit is past every critical point.
        if b0 == points.len() {
            b0 -= 1;
            if !close_enough(points[b0], b) {
                res += self.integrate_piece(f, self.above(points[b0]), b)?;
            }
        }
        // Interior pieces between consecutive critical points, each nudged in on
        // both ends and clipped to the upper limit.
        for i in a0..b0 {
            res +=
                self.integrate_piece(f, self.above(points[i]), self.below(points[i + 1]).min(b))?;
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
        )
        .unwrap();
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

    // Regression: the endpoint nudge must move off a critical point of any sign,
    // including 0.0. A multiplicative nudge leaves 0.0 unmoved (so both pieces
    // sample the jump) and moves negative points the wrong way. Each jump here
    // must be sampled from the correct side.
    #[test]
    fn nudges_off_zero_and_negative_critical_points() {
        // Jump at 0.0: 1 below, 2 at/above. int over [-1, 1] = 1 + 2 = 3.
        let at_zero =
            PiecewiseIntegral::new(SegmentIntegral::new(1).unwrap(), vec![0.0], true).unwrap();
        let f0 = |t: Real| if t < 0.0 { 1.0 } else { 2.0 };
        let got = at_zero.integrate(f0, -1.0, 1.0).unwrap();
        assert!(close(got, 3.0), "zero breakpoint: got {got}");

        // Jump at -2.0: 1 below, 3 at/above. int over [-3, 0] = 1 + 3*2 = 7.
        let at_neg =
            PiecewiseIntegral::new(SegmentIntegral::new(1).unwrap(), vec![-2.0], true).unwrap();
        let fn2 = |t: Real| if t < -2.0 { 1.0 } else { 3.0 };
        let got = at_neg.integrate(fn2, -3.0, 0.0).unwrap();
        assert!(close(got, 7.0), "negative breakpoint: got {got}");
    }

    // The shared driver still governs the outer interval: reversed limits negate
    // and a degenerate interval integrates to zero.
    #[test]
    fn reversed_and_degenerate_outer_limits() {
        let piecewise = PiecewiseIntegral::new(
            SegmentIntegral::new(1).unwrap(),
            vec![1.0, 2.0, 3.0, 4.0, 5.0],
            true,
        )
        .unwrap();
        assert_eq!(piecewise.integrate(step, 2.0, 2.0).unwrap(), 0.0);
        // int_2.5^0 = -int_0^2.5 = -4.5.
        assert!(close(piecewise.integrate(step, 2.5, 0.0).unwrap(), -4.5));
    }

    // With no critical points every call is a single delegated integration.
    #[test]
    fn no_critical_points_delegates_whole_interval() {
        let piecewise =
            PiecewiseIntegral::new(SegmentIntegral::new(1000).unwrap(), vec![], true).unwrap();
        assert!(close(piecewise.integrate(|x| x, 0.0, 2.0).unwrap(), 2.0));
    }

    // The constructor rejects non-finite critical points (invalid-f64 hardening).
    #[test]
    fn non_finite_critical_points_rejected() {
        for bad in [Real::NAN, Real::INFINITY, Real::NEG_INFINITY] {
            assert!(
                PiecewiseIntegral::new(SegmentIntegral::new(1).unwrap(), vec![1.0, bad], true)
                    .is_err(),
                "critical point {bad} should be rejected"
            );
        }
    }
}
