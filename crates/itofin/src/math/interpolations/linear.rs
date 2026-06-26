//! Linear interpolation between discrete points.
//!
//! Port of `ql/math/interpolations/linearinterpolation.hpp`. Segment slopes and
//! a cumulative primitive are precomputed at construction; `locate` clamps to
//! the end segments so enabling extrapolation extends the boundary lines.

use crate::errors::QlResult;
use crate::fail;
use crate::math::interpolations::Interpolation;
use crate::types::{Real, Size};

/// Piecewise-linear interpolation over strictly increasing `x` nodes.
pub struct LinearInterpolation {
    x: Vec<Real>,
    y: Vec<Real>,
    slopes: Vec<Real>,
    primitive_const: Vec<Real>,
    allow_extrapolation: bool,
}

impl LinearInterpolation {
    /// Builds an interpolation through `(x, y)`. The `x` values must be strictly
    /// increasing, and there must be at least two points.
    pub fn new(x: Vec<Real>, y: Vec<Real>) -> QlResult<Self> {
        if x.len() != y.len() {
            fail!(
                "x and y must have equal length ({} vs {})",
                x.len(),
                y.len()
            );
        }
        if x.len() < 2 {
            fail!(
                "linear interpolation needs at least 2 points, got {}",
                x.len()
            );
        }
        for w in x.windows(2) {
            if w[1] <= w[0] {
                fail!("x values must be strictly increasing");
            }
        }

        let n = x.len();
        let mut slopes = vec![0.0; n - 1];
        let mut primitive_const = vec![0.0; n];
        for i in 1..n {
            let dx = x[i] - x[i - 1];
            slopes[i - 1] = (y[i] - y[i - 1]) / dx;
            primitive_const[i] =
                primitive_const[i - 1] + dx * (y[i - 1] + 0.5 * dx * slopes[i - 1]);
        }

        Ok(LinearInterpolation {
            x,
            y,
            slopes,
            primitive_const,
            allow_extrapolation: false,
        })
    }

    /// Sets whether evaluation outside `[x_min, x_max]` is permitted (extending
    /// the end segments) rather than an error.
    pub fn with_extrapolation(mut self, allow: bool) -> Self {
        self.allow_extrapolation = allow;
        self
    }

    /// Whether extrapolation is currently permitted.
    pub fn allows_extrapolation(&self) -> bool {
        self.allow_extrapolation
    }

    /// The index of the segment containing `x`, clamped to the end segments.
    fn locate(&self, x: Real) -> Size {
        let n = self.x.len();
        if x < self.x[0] {
            0
        } else if x > self.x[n - 1] {
            n - 2
        } else {
            self.x[..n - 1].partition_point(|&xi| xi <= x) - 1
        }
    }

    fn check_range(&self, x: Real) -> QlResult<()> {
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

impl Interpolation for LinearInterpolation {
    fn value(&self, x: Real) -> QlResult<Real> {
        self.check_range(x)?;
        let i = self.locate(x);
        Ok(self.y[i] + (x - self.x[i]) * self.slopes[i])
    }

    fn derivative(&self, x: Real) -> QlResult<Real> {
        self.check_range(x)?;
        Ok(self.slopes[self.locate(x)])
    }

    fn primitive(&self, x: Real) -> QlResult<Real> {
        self.check_range(x)?;
        let i = self.locate(x);
        let dx = x - self.x[i];
        Ok(self.primitive_const[i] + dx * (self.y[i] + 0.5 * dx * self.slopes[i]))
    }

    fn x_min(&self) -> Real {
        self.x[0]
    }

    fn x_max(&self) -> Real {
        self.x[self.x.len() - 1]
    }

    fn is_in_range(&self, x: Real) -> bool {
        x >= self.x_min() && x <= self.x_max()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // x: 0, 1, 3 ; y: 0, 2, 2  →  slopes: 2 (on [0,1]), 0 (on [1,3])
    fn sample() -> LinearInterpolation {
        LinearInterpolation::new(vec![0.0, 1.0, 3.0], vec![0.0, 2.0, 2.0]).unwrap()
    }

    #[test]
    fn value_at_nodes_and_between() {
        let f = sample();
        assert_eq!(f.value(0.0).unwrap(), 0.0);
        assert_eq!(f.value(1.0).unwrap(), 2.0);
        assert_eq!(f.value(3.0).unwrap(), 2.0);
        assert_eq!(f.value(0.5).unwrap(), 1.0); // halfway up the slope-2 segment
        assert_eq!(f.value(2.0).unwrap(), 2.0); // on the flat segment
    }

    #[test]
    fn derivative_is_segment_slope() {
        let f = sample();
        assert_eq!(f.derivative(0.5).unwrap(), 2.0);
        assert_eq!(f.derivative(2.0).unwrap(), 0.0);
    }

    #[test]
    fn primitive_is_cumulative_area() {
        let f = sample();
        assert_eq!(f.primitive(0.0).unwrap(), 0.0);
        assert_eq!(f.primitive(1.0).unwrap(), 1.0); // triangle under 0→2 on [0,1]
        assert_eq!(f.primitive(3.0).unwrap(), 5.0); // + 2×2 flat area on [1,3]
        assert!((f.primitive(0.5).unwrap() - 0.25).abs() < 1e-12); // ∫₀^0.5 2x dx
    }

    #[test]
    fn domain_and_in_range() {
        let f = sample();
        assert_eq!(f.x_min(), 0.0);
        assert_eq!(f.x_max(), 3.0);
        assert!(f.is_in_range(1.5));
        assert!(!f.is_in_range(-0.1));
        assert!(!f.is_in_range(3.1));
    }

    #[test]
    fn extrapolation_disabled_errors_out_of_range() {
        let f = sample();
        assert!(f.value(-1.0).is_err());
        assert!(f.value(4.0).is_err());
    }

    #[test]
    fn extrapolation_enabled_extends_end_segments() {
        let f = sample().with_extrapolation(true);
        assert!(f.allows_extrapolation());
        assert_eq!(f.value(-1.0).unwrap(), -2.0); // extends slope-2 segment below
        assert_eq!(f.value(4.0).unwrap(), 2.0); // extends flat segment above
    }

    #[test]
    fn invalid_inputs_rejected() {
        assert!(LinearInterpolation::new(vec![0.0, 1.0], vec![0.0]).is_err());
        assert!(LinearInterpolation::new(vec![0.0], vec![0.0]).is_err());
        assert!(LinearInterpolation::new(vec![0.0, 0.0], vec![1.0, 2.0]).is_err());
    }
}
