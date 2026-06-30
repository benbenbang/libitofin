//! Piecewise-constant (flat) interpolations.
//!
//! Port of `ql/math/interpolations/{backwardflat,forwardflat}interpolation.hpp`.
//! Both step between nodes; they differ only in which node's value a segment
//! takes: [`BackwardFlatInterpolation`] uses the node at or to the right of `x`,
//! [`ForwardFlatInterpolation`] the node at or to the left. A cumulative
//! primitive is precomputed; the derivative is everywhere zero.

use crate::errors::QlResult;
use crate::fail;
use crate::math::interpolations::Interpolation;
use crate::types::{Real, Size};

/// Validate the `(x, y)` nodes: equal length of at least `min_points`, finite,
/// strictly increasing `x`. QuantLib requires 1 point for backward-flat and 2 for
/// forward-flat (`requiredPoints`), so the minimum is passed in.
fn validate(x: &[Real], y: &[Real], min_points: usize) -> QlResult<()> {
    if x.len() != y.len() {
        fail!(
            "x and y must have equal length ({} vs {})",
            x.len(),
            y.len()
        );
    }
    if x.len() < min_points {
        fail!(
            "flat interpolation needs at least {min_points} point(s), got {}",
            x.len()
        );
    }
    for &xi in x {
        if !xi.is_finite() {
            fail!("x values must be finite, got {xi}");
        }
    }
    for &yi in y {
        if !yi.is_finite() {
            fail!("y values must be finite, got {yi}");
        }
    }
    for w in x.windows(2) {
        if w[1] <= w[0] {
            fail!("x values must be strictly increasing");
        }
    }
    Ok(())
}

/// The index of the segment containing `at`, clamped to the end segments
/// (requires at least two nodes).
fn locate(x: &[Real], at: Real) -> Size {
    let n = x.len();
    if at < x[0] {
        0
    } else if at > x[n - 1] {
        n - 2
    } else {
        x[..n - 1].partition_point(|&xi| xi <= at) - 1
    }
}

/// Reject `NaN`, and out-of-range evaluation when extrapolation is disabled.
fn check_range(x: &[Real], allow_extrapolation: bool, at: Real) -> QlResult<()> {
    if at.is_nan() {
        fail!("interpolation cannot be evaluated at NaN");
    }
    let (lo, hi) = (x[0], x[x.len() - 1]);
    if !allow_extrapolation && !(lo..=hi).contains(&at) {
        fail!("interpolation range is [{lo}, {hi}]: extrapolation at {at} not allowed");
    }
    Ok(())
}

/// Backward-flat interpolation: each segment takes the value of the node at or
/// to the right of `x` (so `f(x) = y[i]` at a node `x[i]`, and `y[i+1]` across
/// the open interval `(x[i], x[i+1])`).
pub struct BackwardFlatInterpolation {
    x: Vec<Real>,
    y: Vec<Real>,
    primitive: Vec<Real>,
    allow_extrapolation: bool,
}

impl BackwardFlatInterpolation {
    /// Builds a backward-flat interpolation through `(x, y)`; `x` must be
    /// strictly increasing and there must be at least one point.
    pub fn new(x: Vec<Real>, y: Vec<Real>) -> QlResult<Self> {
        validate(&x, &y, 1)?;
        let n = x.len();
        let mut primitive = vec![0.0; n];
        for i in 1..n {
            // each segment (x[i-1], x[i]] carries the right node value y[i]
            primitive[i] = primitive[i - 1] + (x[i] - x[i - 1]) * y[i];
        }
        Ok(BackwardFlatInterpolation {
            x,
            y,
            primitive,
            allow_extrapolation: false,
        })
    }

    /// Sets whether evaluation outside `[x_min, x_max]` is permitted.
    pub fn with_extrapolation(mut self, allow: bool) -> Self {
        self.allow_extrapolation = allow;
        self
    }

    /// Whether extrapolation is currently permitted.
    pub fn allows_extrapolation(&self) -> bool {
        self.allow_extrapolation
    }
}

impl Interpolation for BackwardFlatInterpolation {
    fn value(&self, at: Real) -> QlResult<Real> {
        check_range(&self.x, self.allow_extrapolation, at)?;
        // the node at or to the right of `at`, clamped into range
        let j = self.x.partition_point(|&xi| xi < at).min(self.x.len() - 1);
        Ok(self.y[j])
    }

    fn derivative(&self, at: Real) -> QlResult<Real> {
        check_range(&self.x, self.allow_extrapolation, at)?;
        Ok(0.0)
    }

    fn primitive(&self, at: Real) -> QlResult<Real> {
        check_range(&self.x, self.allow_extrapolation, at)?;
        if self.x.len() == 1 {
            return Ok((at - self.x[0]) * self.y[0]);
        }
        let i = locate(&self.x, at);
        Ok(self.primitive[i] + (at - self.x[i]) * self.y[i + 1])
    }

    fn x_min(&self) -> Real {
        self.x[0]
    }

    fn x_max(&self) -> Real {
        self.x[self.x.len() - 1]
    }

    fn is_in_range(&self, at: Real) -> bool {
        (self.x_min()..=self.x_max()).contains(&at)
    }
}

/// Forward-flat interpolation: each segment takes the value of the node at or to
/// the left of `x` (so `f(x) = y[i]` across `[x[i], x[i+1])`).
pub struct ForwardFlatInterpolation {
    x: Vec<Real>,
    y: Vec<Real>,
    primitive: Vec<Real>,
    allow_extrapolation: bool,
}

impl ForwardFlatInterpolation {
    /// Builds a forward-flat interpolation through `(x, y)`; `x` must be strictly
    /// increasing and there must be at least two points (QuantLib's
    /// `ForwardFlat::requiredPoints`, unlike backward-flat which allows one).
    pub fn new(x: Vec<Real>, y: Vec<Real>) -> QlResult<Self> {
        validate(&x, &y, 2)?;
        let n = x.len();
        let mut primitive = vec![0.0; n];
        for i in 1..n {
            // each segment [x[i-1], x[i]) carries the left node value y[i-1]
            primitive[i] = primitive[i - 1] + (x[i] - x[i - 1]) * y[i - 1];
        }
        Ok(ForwardFlatInterpolation {
            x,
            y,
            primitive,
            allow_extrapolation: false,
        })
    }

    /// Sets whether evaluation outside `[x_min, x_max]` is permitted.
    pub fn with_extrapolation(mut self, allow: bool) -> Self {
        self.allow_extrapolation = allow;
        self
    }

    /// Whether extrapolation is currently permitted.
    pub fn allows_extrapolation(&self) -> bool {
        self.allow_extrapolation
    }
}

impl Interpolation for ForwardFlatInterpolation {
    fn value(&self, at: Real) -> QlResult<Real> {
        check_range(&self.x, self.allow_extrapolation, at)?;
        // the node at or to the left of `at`, clamped into range
        let j = self.x.partition_point(|&xi| xi <= at).saturating_sub(1);
        Ok(self.y[j])
    }

    fn derivative(&self, at: Real) -> QlResult<Real> {
        check_range(&self.x, self.allow_extrapolation, at)?;
        Ok(0.0)
    }

    fn primitive(&self, at: Real) -> QlResult<Real> {
        check_range(&self.x, self.allow_extrapolation, at)?;
        if self.x.len() == 1 {
            return Ok((at - self.x[0]) * self.y[0]);
        }
        let i = locate(&self.x, at);
        Ok(self.primitive[i] + (at - self.x[i]) * self.y[i])
    }

    fn x_min(&self) -> Real {
        self.x[0]
    }

    fn x_max(&self) -> Real {
        self.x[self.x.len() - 1]
    }

    fn is_in_range(&self, at: Real) -> bool {
        (self.x_min()..=self.x_max()).contains(&at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Port of testBackwardFlat: x={0..4}, y={5,4,3,2,1}.
    #[test]
    fn backward_flat_matches_oracle() {
        let x = vec![0.0, 1.0, 2.0, 3.0, 4.0];
        let y = vec![5.0, 4.0, 3.0, 2.0, 1.0];
        let f = BackwardFlatInterpolation::new(x.clone(), y.clone()).unwrap();
        // at the nodes the value is the node value
        for i in 0..x.len() {
            assert_eq!(f.value(x[i]).unwrap(), y[i]);
        }
        // across a segment the value is the right node
        for i in 0..x.len() - 1 {
            assert_eq!(f.value(0.5 * (x[i] + x[i + 1])).unwrap(), y[i + 1]);
        }
        // extrapolation holds the end values flat
        let f = f.with_extrapolation(true);
        assert_eq!(f.value(x[0] - 0.5).unwrap(), y[0]);
        assert_eq!(f.value(x[4] + 0.5).unwrap(), y[4]);
    }

    // Port of testForwardFlat: same data, but a segment takes the left node.
    #[test]
    fn forward_flat_matches_oracle() {
        let x = vec![0.0, 1.0, 2.0, 3.0, 4.0];
        let y = vec![5.0, 4.0, 3.0, 2.0, 1.0];
        let f = ForwardFlatInterpolation::new(x.clone(), y.clone()).unwrap();
        for i in 0..x.len() {
            assert_eq!(f.value(x[i]).unwrap(), y[i]);
        }
        for i in 0..x.len() - 1 {
            assert_eq!(f.value(0.5 * (x[i] + x[i + 1])).unwrap(), y[i]);
        }
        let f = f.with_extrapolation(true);
        assert_eq!(f.value(x[0] - 0.5).unwrap(), y[0]);
        assert_eq!(f.value(x[4] + 0.5).unwrap(), y[4]);
    }

    #[test]
    fn derivative_is_zero_and_primitive_is_cumulative() {
        // y = 5,4,3,2,1 on x = 0..4.
        let x = vec![0.0, 1.0, 2.0, 3.0, 4.0];
        let y = vec![5.0, 4.0, 3.0, 2.0, 1.0];
        let b = BackwardFlatInterpolation::new(x.clone(), y.clone()).unwrap();
        let fwd = ForwardFlatInterpolation::new(x.clone(), y.clone()).unwrap();
        assert_eq!(b.derivative(2.5).unwrap(), 0.0);
        assert_eq!(fwd.derivative(2.5).unwrap(), 0.0);
        // Backward: a segment's area uses its right node. Over [0,1] that is
        // y[1]=4, and at 1.5 we add half of y[2]=3.
        assert_eq!(b.primitive(0.0).unwrap(), 0.0);
        assert_eq!(b.primitive(1.0).unwrap(), 4.0);
        assert_eq!(b.primitive(1.5).unwrap(), 4.0 + 0.5 * 3.0);
        // Forward: a segment's area uses its left node. Over [0,1] that is
        // y[0]=5, and at 1.5 we add half of y[1]=4.
        assert_eq!(fwd.primitive(1.0).unwrap(), 5.0);
        assert_eq!(fwd.primitive(1.5).unwrap(), 5.0 + 0.5 * 4.0);
    }

    // Port of testBackwardFlatOnSinglePoint: knot 1.0, value 2.5. Only backward-
    // flat is defined on a single point; off-node evaluation is extrapolation and
    // the primitive grows linearly. Forward-flat requires two points.
    #[test]
    fn backward_flat_single_point_is_piecewise_constant() {
        let b = BackwardFlatInterpolation::new(vec![1.0], vec![2.5])
            .unwrap()
            .with_extrapolation(true);
        for at in [-1.0, 1.0, 2.0, 3.0] {
            assert_eq!(b.value(at).unwrap(), 2.5);
            assert_eq!(b.primitive(at).unwrap(), 2.5 * (at - 1.0));
        }
        // forward-flat needs at least two points (QuantLib parity)
        assert!(ForwardFlatInterpolation::new(vec![1.0], vec![2.5]).is_err());
    }

    #[test]
    fn nan_input_is_rejected() {
        let f = BackwardFlatInterpolation::new(vec![0.0, 1.0], vec![1.0, 2.0])
            .unwrap()
            .with_extrapolation(true);
        assert!(f.value(Real::NAN).is_err());
        assert!(f.primitive(Real::NAN).is_err());
    }

    #[test]
    fn out_of_range_errors_without_extrapolation() {
        let f = ForwardFlatInterpolation::new(vec![0.0, 1.0], vec![1.0, 2.0]).unwrap();
        assert!(f.value(-0.5).is_err());
        assert!(f.value(1.5).is_err());
    }

    #[test]
    fn invalid_inputs_rejected() {
        assert!(BackwardFlatInterpolation::new(vec![], vec![]).is_err());
        assert!(BackwardFlatInterpolation::new(vec![0.0, 1.0], vec![0.0]).is_err());
        assert!(BackwardFlatInterpolation::new(vec![1.0, 0.0], vec![0.0, 1.0]).is_err());
        assert!(ForwardFlatInterpolation::new(vec![0.0, Real::NAN], vec![0.0, 1.0]).is_err());
    }
}
