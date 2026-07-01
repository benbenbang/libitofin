//! Cubic interpolation between discrete points.
//!
//! Port of `CubicInterpolation` from
//! `ql/math/interpolations/cubicinterpolation.hpp`. On each segment
//! `P_i(x) = y_i + a_i (x-x_i) + b_i (x-x_i)^2 + c_i (x-x_i)^3`, with the node
//! first-derivatives supplied by a [`CubicDerivativeApprox`] scheme. This layer
//! ports the coefficient/evaluation engine and the two simplest local schemes,
//! Parabolic and Kruger. The non-local spline schemes and the Hyman
//! monotonicity filter land in later layers; a private `CubicConfig` already
//! carries their knobs so those layers slot in without reshaping the pipeline.

use crate::errors::QlResult;
use crate::fail;
use crate::math::interpolations::Interpolation;
use crate::types::{Real, Size};

/// First-derivative approximation scheme. Variants are added as they are ported,
/// so it is `#[non_exhaustive]`: downstream matches must include a wildcard arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CubicDerivativeApprox {
    /// Parabolic approximation (local, non-monotonic): fits a local parabola,
    /// so it reproduces quadratic data exactly.
    Parabolic,
    /// Kruger approximation (local, monotonic): a harmonic-style blend that
    /// zeroes the derivative where the slope changes sign.
    Kruger,
}

/// Boundary condition for the (non-local) spline schemes. Its shape is final;
/// only the default is exercised until the spline layer consumes the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
enum CubicBoundaryCondition {
    NotAKnot,
    FirstDerivative,
    SecondDerivative,
    Periodic,
    Lagrange,
}

/// The full cubic configuration. Its shape is final from day one; `monotonic`
/// and the boundary conditions are inert until the Hyman and spline layers
/// consume them, so they are allowed to sit unread for now.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
struct CubicConfig {
    da: CubicDerivativeApprox,
    monotonic: bool,
    left_cond: CubicBoundaryCondition,
    left_value: Real,
    right_cond: CubicBoundaryCondition,
    right_value: Real,
}

impl CubicConfig {
    /// The QuantLib `Cubic` defaults, specialized to a derivative scheme.
    fn local(da: CubicDerivativeApprox) -> Self {
        CubicConfig {
            da,
            monotonic: false,
            left_cond: CubicBoundaryCondition::SecondDerivative,
            left_value: 0.0,
            right_cond: CubicBoundaryCondition::SecondDerivative,
            right_value: 0.0,
        }
    }
}

/// Piecewise-cubic interpolation over strictly increasing `x` nodes.
pub struct CubicInterpolation {
    x: Vec<Real>,
    y: Vec<Real>,
    a: Vec<Real>,
    b: Vec<Real>,
    c: Vec<Real>,
    primitive_const: Vec<Real>,
    allow_extrapolation: bool,
}

impl CubicInterpolation {
    /// Builds a cubic interpolation through `(x, y)` using the derivative scheme
    /// `da`. The `x` values must be strictly increasing with at least two
    /// points.
    pub fn new(x: Vec<Real>, y: Vec<Real>, da: CubicDerivativeApprox) -> QlResult<Self> {
        Self::build(x, y, CubicConfig::local(da))
    }

    fn build(x: Vec<Real>, y: Vec<Real>, config: CubicConfig) -> QlResult<Self> {
        if x.len() != y.len() {
            fail!(
                "x and y must have equal length ({} vs {})",
                x.len(),
                y.len()
            );
        }
        if x.len() < 2 {
            fail!(
                "cubic interpolation needs at least 2 points, got {}",
                x.len()
            );
        }
        for &xi in &x {
            if !xi.is_finite() {
                fail!("x values must be finite, got {xi}");
            }
        }
        for &yi in &y {
            if !yi.is_finite() {
                fail!("y values must be finite, got {yi}");
            }
        }
        for w in x.windows(2) {
            if w[1] <= w[0] {
                fail!("x values must be strictly increasing");
            }
        }

        let n = x.len();
        let mut dx = vec![0.0; n - 1];
        let mut s = vec![0.0; n - 1];
        for i in 0..n - 1 {
            dx[i] = x[i + 1] - x[i];
            s[i] = (y[i + 1] - y[i]) / dx[i];
        }

        let d = node_derivatives(config.da, &dx, &s);

        let mut a = vec![0.0; n - 1];
        let mut b = vec![0.0; n - 1];
        let mut c = vec![0.0; n - 1];
        for i in 0..n - 1 {
            a[i] = d[i];
            b[i] = (3.0 * s[i] - d[i + 1] - 2.0 * d[i]) / dx[i];
            c[i] = (d[i + 1] + d[i] - 2.0 * s[i]) / (dx[i] * dx[i]);
        }

        let mut primitive_const = vec![0.0; n - 1];
        for i in 1..n - 1 {
            primitive_const[i] = primitive_const[i - 1]
                + dx[i - 1]
                    * (y[i - 1]
                        + dx[i - 1]
                            * (a[i - 1] / 2.0
                                + dx[i - 1] * (b[i - 1] / 3.0 + dx[i - 1] * c[i - 1] / 4.0)));
        }

        Ok(CubicInterpolation {
            x,
            y,
            a,
            b,
            c,
            primitive_const,
            allow_extrapolation: false,
        })
    }

    /// Sets whether evaluation outside `[x_min, x_max]` is permitted (extending
    /// the end segments' cubics) rather than an error.
    pub fn with_extrapolation(mut self, allow: bool) -> Self {
        self.allow_extrapolation = allow;
        self
    }

    /// Whether extrapolation is currently permitted.
    pub fn allows_extrapolation(&self) -> bool {
        self.allow_extrapolation
    }

    /// The second derivative at `x`.
    pub fn second_derivative(&self, x: Real) -> QlResult<Real> {
        self.check_range(x)?;
        let j = self.locate(x);
        let dx = x - self.x[j];
        Ok(2.0 * self.b[j] + 6.0 * self.c[j] * dx)
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

/// The node first-derivatives for a local scheme, given segment widths `dx` and
/// secant slopes `s` (both length `n-1`). Returns a length-`n` vector.
fn node_derivatives(da: CubicDerivativeApprox, dx: &[Real], s: &[Real]) -> Vec<Real> {
    let n = dx.len() + 1;
    // Two points degenerate to the single secant slope for every scheme.
    if n == 2 {
        return vec![s[0], s[0]];
    }

    let mut d = vec![0.0; n];
    match da {
        CubicDerivativeApprox::Parabolic => {
            for i in 1..n - 1 {
                d[i] = (dx[i - 1] * s[i] + dx[i] * s[i - 1]) / (dx[i] + dx[i - 1]);
            }
            d[0] = ((2.0 * dx[0] + dx[1]) * s[0] - dx[0] * s[1]) / (dx[0] + dx[1]);
            d[n - 1] = ((2.0 * dx[n - 2] + dx[n - 3]) * s[n - 2] - dx[n - 2] * s[n - 3])
                / (dx[n - 2] + dx[n - 3]);
        }
        CubicDerivativeApprox::Kruger => {
            for i in 1..n - 1 {
                d[i] = if s[i - 1] * s[i] < 0.0 {
                    // slope changes sign at the point
                    0.0
                } else {
                    2.0 / (1.0 / s[i - 1] + 1.0 / s[i])
                };
            }
            d[0] = (3.0 * s[0] - d[1]) / 2.0;
            d[n - 1] = (3.0 * s[n - 2] - d[n - 2]) / 2.0;
        }
    }
    d
}

impl Interpolation for CubicInterpolation {
    fn value(&self, x: Real) -> QlResult<Real> {
        self.check_range(x)?;
        let j = self.locate(x);
        let dx = x - self.x[j];
        Ok(self.y[j] + dx * (self.a[j] + dx * (self.b[j] + dx * self.c[j])))
    }

    fn derivative(&self, x: Real) -> QlResult<Real> {
        self.check_range(x)?;
        let j = self.locate(x);
        let dx = x - self.x[j];
        Ok(self.a[j] + (2.0 * self.b[j] + 3.0 * self.c[j] * dx) * dx)
    }

    fn primitive(&self, x: Real) -> QlResult<Real> {
        self.check_range(x)?;
        let j = self.locate(x);
        let dx = x - self.x[j];
        Ok(self.primitive_const[j]
            + dx * (self.y[j]
                + dx * (self.a[j] / 2.0 + dx * (self.b[j] / 3.0 + dx * self.c[j] / 4.0))))
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

/// Parabolic cubic interpolation (local, non-monotonic).
pub struct ParabolicInterpolation;

impl ParabolicInterpolation {
    /// Builds a parabolic cubic interpolation through `(x, y)`.
    // A factory for the underlying CubicInterpolation, mirroring QuantLib's
    // convenience subclasses, so it deliberately does not return Self.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(x: Vec<Real>, y: Vec<Real>) -> QlResult<CubicInterpolation> {
        CubicInterpolation::new(x, y, CubicDerivativeApprox::Parabolic)
    }
}

/// Kruger cubic interpolation (local, monotonic).
pub struct KrugerCubicInterpolation;

impl KrugerCubicInterpolation {
    /// Builds a Kruger cubic interpolation through `(x, y)`.
    // A factory for the underlying CubicInterpolation, mirroring QuantLib's
    // convenience subclasses, so it deliberately does not return Self.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(x: Vec<Real>, y: Vec<Real>) -> QlResult<CubicInterpolation> {
        CubicInterpolation::new(x, y, CubicDerivativeApprox::Kruger)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Parabolic reproduces q(x) = 1 + 2x + 3x^2 exactly. Non-uniform nodes.
    fn q(x: Real) -> Real {
        1.0 + 2.0 * x + 3.0 * x * x
    }

    fn parabolic_sample() -> CubicInterpolation {
        let x = vec![0.0, 1.0, 3.0, 4.0];
        let y = x.iter().map(|&xi| q(xi)).collect();
        ParabolicInterpolation::new(x, y).unwrap()
    }

    fn assert_close(got: Real, expected: Real) {
        let tol = 1e-11 * (1.0 + expected.abs());
        assert!(
            (got - expected).abs() <= tol,
            "got {got}, expected {expected}"
        );
    }

    #[test]
    fn parabolic_reproduces_quadratic() {
        // value = q, derivative = q' = 2 + 6x, second derivative = q'' = 6, and
        // the primitive from x_min = 0 is x + x^2 + x^3.
        let f = parabolic_sample();
        for &x in &[0.0, 0.5, 1.0, 2.0, 3.0, 3.7, 4.0_f64] {
            assert_close(f.value(x).unwrap(), q(x));
            assert_close(f.derivative(x).unwrap(), 2.0 + 6.0 * x);
            assert_close(f.second_derivative(x).unwrap(), 6.0);
            assert_close(f.primitive(x).unwrap(), x + x * x + x * x * x);
        }
    }

    #[test]
    fn two_points_is_linear() {
        // n == 2 returns the single secant slope before the scheme match, so it
        // is scheme-independent. y = [1, 5] over [0, 2]: value = 1 + 2x.
        let f = ParabolicInterpolation::new(vec![0.0, 2.0], vec![1.0, 5.0]).unwrap();
        assert_close(f.value(1.0).unwrap(), 3.0);
        assert_close(f.derivative(1.5).unwrap(), 2.0);
        assert_close(f.second_derivative(0.7).unwrap(), 0.0);
    }

    #[test]
    fn kruger_node_derivatives() {
        // y = [0, 1, 0]: passes through the nodes; S[0]=1, S[1]=-1 (product < 0)
        // so the Kruger derivative at the peak is 0.
        let f = KrugerCubicInterpolation::new(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 0.0]).unwrap();
        assert_close(f.value(1.0).unwrap(), 1.0);
        assert_close(f.derivative(1.0).unwrap(), 0.0);
        // y = [0, 0.5, 2]: S = [0.5, 1.5] same sign, so the interior derivative
        // is the harmonic mean 2/(1/0.5 + 1/1.5) = 0.75.
        let g = KrugerCubicInterpolation::new(vec![0.0, 1.0, 2.0], vec![0.0, 0.5, 2.0]).unwrap();
        assert_close(g.derivative(1.0).unwrap(), 0.75);
    }

    #[test]
    fn domain_range_and_extrapolation() {
        let f = parabolic_sample();
        assert_eq!(f.x_min(), 0.0);
        assert_eq!(f.x_max(), 4.0);
        assert!(f.is_in_range(2.0));
        assert!(!f.is_in_range(-0.1));
        assert!(f.value(-1.0).is_err());
        assert!(f.value(5.0).is_err());

        // With extrapolation the end segment's cubic (still q) extends exactly.
        let g = parabolic_sample().with_extrapolation(true);
        assert!(g.allows_extrapolation());
        assert_close(g.value(5.0).unwrap(), q(5.0));
        assert_close(g.value(-1.0).unwrap(), q(-1.0));
    }

    #[test]
    fn rejects_nan_eval_and_bad_construction() {
        let f = parabolic_sample().with_extrapolation(true);
        assert!(f.value(Real::NAN).is_err());
        assert!(f.derivative(Real::NAN).is_err());
        assert!(f.second_derivative(Real::NAN).is_err());

        let da = CubicDerivativeApprox::Parabolic;
        assert!(CubicInterpolation::new(vec![0.0], vec![1.0], da).is_err());
        assert!(CubicInterpolation::new(vec![0.0, 1.0], vec![1.0], da).is_err());
        assert!(CubicInterpolation::new(vec![1.0, 1.0], vec![1.0, 2.0], da).is_err());
        assert!(CubicInterpolation::new(vec![0.0, Real::NAN], vec![1.0, 2.0], da).is_err());
    }
}
