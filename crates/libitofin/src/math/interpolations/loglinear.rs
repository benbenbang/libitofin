//! Log-linear interpolation between discrete points.
//!
//! Port of `LogLinearInterpolation` from
//! `ql/math/interpolations/loginterpolation.hpp`: linear interpolation of
//! `ln(y)`, exponentiated back. Equivalently a geometric interpolation,
//! `y(x) = y_i^{1-t} y_{i+1}^{t}` on each segment, so all `y` values must be
//! strictly positive. QuantLib leaves the antiderivative unimplemented
//! (`QL_FAIL`), which we surface as an error from [`primitive`](Interpolation::primitive).

use crate::errors::QlResult;
use crate::fail;
use crate::math::interpolations::linear::LinearInterpolation;
use crate::math::interpolations::{Interpolation, Interpolator};
use crate::types::Real;

/// Factory for [`LogLinearInterpolation`] (QuantLib's `LogLinear` traits
/// class).
#[derive(Clone, Copy, Default)]
pub struct LogLinear;

impl Interpolator for LogLinear {
    type Output = LogLinearInterpolation;

    fn interpolate(&self, x: &[Real], y: &[Real]) -> QlResult<LogLinearInterpolation> {
        LogLinearInterpolation::new(x.to_vec(), y.to_vec())
    }
}

/// Log-linear (geometric) interpolation over strictly increasing `x` nodes.
pub struct LogLinearInterpolation {
    /// Linear interpolation of `ln(y)`; built with extrapolation enabled so the
    /// range policy is governed solely by this type's `allow_extrapolation`.
    log_linear: LinearInterpolation,
    allow_extrapolation: bool,
}

impl LogLinearInterpolation {
    /// Builds an interpolation through `(x, y)`. The `x` values must be strictly
    /// increasing with at least two points, and every `y` must be strictly
    /// positive (the interpolation lives in log space).
    pub fn new(x: Vec<Real>, y: Vec<Real>) -> QlResult<Self> {
        for (i, &yi) in y.iter().enumerate() {
            if yi.is_nan() || yi <= 0.0 {
                fail!("log-linear interpolation requires positive y values, got y[{i}] = {yi}");
            }
        }
        let log_y = y.iter().map(|&yi| yi.ln()).collect();
        // Remaining validation (equal length, >= 2 points, finite and strictly
        // increasing x, finite log y) is delegated to LinearInterpolation.
        let log_linear = LinearInterpolation::new(x, log_y)?.with_extrapolation(true);
        Ok(LogLinearInterpolation {
            log_linear,
            allow_extrapolation: false,
        })
    }

    /// Sets whether evaluation outside `[x_min, x_max]` is permitted (extending
    /// the end segments geometrically) rather than an error.
    pub fn with_extrapolation(mut self, allow: bool) -> Self {
        self.allow_extrapolation = allow;
        self
    }

    /// Whether extrapolation is currently permitted.
    pub fn allows_extrapolation(&self) -> bool {
        self.allow_extrapolation
    }

    fn check_range(&self, x: Real) -> QlResult<()> {
        if x.is_nan() {
            fail!("interpolation cannot be evaluated at NaN");
        }
        if !self.allow_extrapolation && !self.is_in_range(x) {
            fail!(
                "interpolation range is [{}, {}]: extrapolation at {x} not allowed",
                self.x_min(),
                self.x_max()
            );
        }
        Ok(())
    }
}

impl Interpolation for LogLinearInterpolation {
    fn value(&self, x: Real) -> QlResult<Real> {
        self.check_range(x)?;
        // log_linear extrapolates internally, so this never re-checks the range.
        Ok(self.log_linear.value(x)?.exp())
    }

    fn derivative(&self, x: Real) -> QlResult<Real> {
        self.check_range(x)?;
        // d/dx exp(L(x)) = exp(L(x)) * L'(x), with L the linear-in-log fit.
        Ok(self.log_linear.value(x)?.exp() * self.log_linear.derivative(x)?)
    }

    fn primitive(&self, _x: Real) -> QlResult<Real> {
        // Matches QuantLib's LogInterpolationImpl::primitive (QL_FAIL): the
        // antiderivative of an exponential-of-piecewise-linear has no
        // closed form the library implements.
        fail!("log-linear interpolation primitive is not implemented")
    }

    fn x_min(&self) -> Real {
        self.log_linear.x_min()
    }

    fn x_max(&self) -> Real {
        self.log_linear.x_max()
    }

    fn is_in_range(&self, x: Real) -> bool {
        self.log_linear.is_in_range(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::E;

    // y = exp(x): ln(y) = x is exactly linear, so value recovers exp exactly.
    fn exp_sample() -> LogLinearInterpolation {
        LogLinearInterpolation::new(vec![0.0, 1.0, 2.0], vec![1.0, E, E * E]).unwrap()
    }

    fn assert_close(got: Real, expected: Real) {
        let tol = 1e-12 * (1.0 + expected.abs());
        assert!(
            (got - expected).abs() <= tol,
            "got {got}, expected {expected}"
        );
    }

    #[test]
    fn value_at_nodes_returns_y() {
        let f = exp_sample();
        assert_close(f.value(0.0).unwrap(), 1.0);
        assert_close(f.value(1.0).unwrap(), E);
        assert_close(f.value(2.0).unwrap(), E * E);
    }

    #[test]
    fn value_recovers_exp_between_nodes() {
        let f = exp_sample();
        for &x in &[0.25, 0.5, 1.3, 1.75_f64] {
            assert_close(f.value(x).unwrap(), x.exp());
        }
    }

    #[test]
    fn midpoint_is_geometric_mean() {
        // x = [0, 2], y = [1, 9]: at x = 1, value = sqrt(1*9) = 3.
        let f = LogLinearInterpolation::new(vec![0.0, 2.0], vec![1.0, 9.0]).unwrap();
        assert_close(f.value(1.0).unwrap(), 3.0);
    }

    #[test]
    fn derivative_is_value_times_log_slope() {
        // For y = exp(x) the log-slope is 1, so derivative = value = exp(x).
        let f = exp_sample();
        for &x in &[0.25, 0.5, 1.75_f64] {
            assert_close(f.derivative(x).unwrap(), x.exp());
        }
        // x = [0, 2], y = [1, 9]: log-slope = ln(9)/2 = ln(3); at x = 1,
        // derivative = 3 * ln(3).
        let g = LogLinearInterpolation::new(vec![0.0, 2.0], vec![1.0, 9.0]).unwrap();
        assert_close(g.derivative(1.0).unwrap(), 3.0 * 3.0_f64.ln());
    }

    #[test]
    fn primitive_is_not_implemented() {
        let f = exp_sample();
        assert!(f.primitive(0.5).is_err());
        assert!(f.primitive(1.0).is_err());
    }

    #[test]
    fn domain_and_in_range() {
        let f = exp_sample();
        assert_eq!(f.x_min(), 0.0);
        assert_eq!(f.x_max(), 2.0);
        assert!(f.is_in_range(1.0));
        assert!(!f.is_in_range(-0.1));
        assert!(!f.is_in_range(2.1));
    }

    #[test]
    fn extrapolation_disabled_errors_out_of_range() {
        let f = exp_sample();
        assert!(f.value(-1.0).is_err());
        assert!(f.value(3.0).is_err());
    }

    #[test]
    fn extrapolation_enabled_extends_geometrically() {
        let f = exp_sample().with_extrapolation(true);
        assert!(f.allows_extrapolation());
        // ln(y) = x extends linearly, so exp is recovered outside the range too.
        assert_close(f.value(-1.0).unwrap(), (-1.0_f64).exp());
        assert_close(f.value(3.0).unwrap(), 3.0_f64.exp());
    }

    #[test]
    fn nan_input_is_rejected() {
        let f = exp_sample().with_extrapolation(true);
        assert!(f.value(Real::NAN).is_err());
        assert!(f.derivative(Real::NAN).is_err());
    }

    #[test]
    fn non_positive_y_rejected() {
        assert!(LogLinearInterpolation::new(vec![0.0, 1.0], vec![1.0, 0.0]).is_err());
        assert!(LogLinearInterpolation::new(vec![0.0, 1.0], vec![1.0, -2.0]).is_err());
        assert!(LogLinearInterpolation::new(vec![0.0, 1.0], vec![Real::NAN, 1.0]).is_err());
    }

    #[test]
    fn factory_builds_the_interpolation() {
        let f = LogLinear.interpolate(&[0.0, 2.0], &[1.0, 9.0]).unwrap();
        assert_close(f.value(1.0).unwrap(), 3.0);
        assert_eq!(LogLinear.required_points(), 2);
        assert!(LogLinear.interpolate(&[0.0, 1.0], &[1.0, -2.0]).is_err());
    }

    #[test]
    fn invalid_x_rejected() {
        // Delegated to LinearInterpolation: too few points, unequal length,
        // non-increasing x.
        assert!(LogLinearInterpolation::new(vec![0.0], vec![1.0]).is_err());
        assert!(LogLinearInterpolation::new(vec![0.0, 1.0], vec![1.0]).is_err());
        assert!(LogLinearInterpolation::new(vec![1.0, 1.0], vec![1.0, 2.0]).is_err());
    }
}
