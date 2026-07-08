//! Bicubic spline interpolation over a 2-D grid.
//!
//! Port of `BicubicSpline` from
//! `ql/math/interpolations/bicubicsplineinterpolation.hpp`: a tensor product of
//! natural cubic splines. One spline is fitted along `x` for each `y` row; a
//! value is obtained by evaluating those row-splines at `x` and fitting a
//! natural cubic spline of the results along `y`. Both directions use the
//! natural (`SecondDerivative` = 0) boundary, matching QuantLib.
//!
//! QuantLib's mutable `update()` has no analogue here: like the other
//! interpolations this type is immutable, so a data change means rebuilding.

use crate::errors::QlResult;
use crate::fail;
use crate::math::interpolations::cubic::{CubicInterpolation, CubicNaturalSpline};
use crate::math::interpolations::{Interpolation, Interpolation2D, Interpolator2D};
use crate::types::Real;

/// Factory for [`BicubicSpline`] (QuantLib's `Bicubic` traits class).
#[derive(Clone, Copy, Default)]
pub struct Bicubic;

impl Interpolator2D for Bicubic {
    type Output = BicubicSpline;

    fn interpolate(
        &self,
        x: Vec<Real>,
        y: Vec<Real>,
        z: Vec<Vec<Real>>,
    ) -> QlResult<BicubicSpline> {
        BicubicSpline::new(x, y, z)
    }
}

/// Bicubic spline interpolation over strictly increasing `x` and `y` node grids.
///
/// `z[j][i]` holds the tabulated value at `(x[i], y[j])`: the outer index runs
/// over `y` (rows), the inner over `x` (columns).
pub struct BicubicSpline {
    x: Vec<Real>,
    y: Vec<Real>,
    /// One natural cubic spline per `y` row, over `x` (extrapolation enabled so
    /// this type owns the range policy).
    row_splines: Vec<CubicInterpolation>,
    allow_extrapolation: bool,
}

impl BicubicSpline {
    /// Builds a bicubic spline over the grid `(x, y)` with values `z`. Both axes
    /// must be strictly increasing with at least two points, and `z` must be a
    /// `y.len()` by `x.len()` matrix of finite values.
    pub fn new(x: Vec<Real>, y: Vec<Real>, z: Vec<Vec<Real>>) -> QlResult<Self> {
        if y.len() < 2 {
            fail!("bicubic spline needs at least 2 y points, got {}", y.len());
        }
        for &yi in &y {
            if !yi.is_finite() {
                fail!("y values must be finite, got {yi}");
            }
        }
        for w in y.windows(2) {
            if w[1] <= w[0] {
                fail!("y values must be strictly increasing");
            }
        }
        if z.len() != y.len() {
            fail!(
                "z must have one row per y node ({} rows vs {} y values)",
                z.len(),
                y.len()
            );
        }
        // Each row spline validates the shared x axis (strictly increasing,
        // finite, at least two points) and its row's finiteness.
        let row_splines = z
            .into_iter()
            .map(|row| Ok(CubicNaturalSpline::new(x.clone(), row)?.with_extrapolation(true)))
            .collect::<QlResult<Vec<_>>>()?;

        Ok(BicubicSpline {
            x,
            y,
            row_splines,
            allow_extrapolation: false,
        })
    }

    /// Sets whether evaluation outside the domain is permitted (extending the
    /// boundary splines) rather than an error.
    pub fn with_extrapolation(mut self, allow: bool) -> Self {
        self.allow_extrapolation = allow;
        self
    }

    /// Whether extrapolation is currently permitted.
    pub fn allows_extrapolation(&self) -> bool {
        self.allow_extrapolation
    }

    /// The `x` first derivative at `(x, y)`.
    pub fn derivative_x(&self, x: Real, y: Real) -> QlResult<Real> {
        self.check_range(x, y)?;
        self.x_spline(self.x_section(|bs, xi| bs.eval(xi, y))?)?
            .derivative(x)
    }

    /// The `x` second derivative at `(x, y)`.
    pub fn second_derivative_x(&self, x: Real, y: Real) -> QlResult<Real> {
        self.check_range(x, y)?;
        self.x_spline(self.x_section(|bs, xi| bs.eval(xi, y))?)?
            .second_derivative(x)
    }

    /// The `y` first derivative at `(x, y)`.
    pub fn derivative_y(&self, x: Real, y: Real) -> QlResult<Real> {
        self.check_range(x, y)?;
        self.column_spline(x)?.derivative(y)
    }

    /// The `y` second derivative at `(x, y)`.
    pub fn second_derivative_y(&self, x: Real, y: Real) -> QlResult<Real> {
        self.check_range(x, y)?;
        self.column_spline(x)?.second_derivative(y)
    }

    /// The mixed `x`-`y` derivative at `(x, y)`.
    pub fn derivative_xy(&self, x: Real, y: Real) -> QlResult<Real> {
        self.check_range(x, y)?;
        self.x_spline(self.x_section(|bs, xi| bs.column_spline(xi)?.derivative(y))?)?
            .derivative(x)
    }

    /// The interpolated value at `(x, y)` without the range check.
    fn eval(&self, x: Real, y: Real) -> QlResult<Real> {
        self.column_spline(x)?.value(y)
    }

    /// The natural cubic spline of the row-splines' values at `x`, over `y`.
    fn column_spline(&self, x: Real) -> QlResult<CubicInterpolation> {
        let section = self
            .row_splines
            .iter()
            .map(|sp| sp.value(x))
            .collect::<QlResult<Vec<_>>>()?;
        Ok(CubicNaturalSpline::new(self.y.clone(), section)?.with_extrapolation(true))
    }

    /// A section of `f` evaluated at each `x` node (with `y` captured by `f`).
    fn x_section<F>(&self, f: F) -> QlResult<Vec<Real>>
    where
        F: Fn(&Self, Real) -> QlResult<Real>,
    {
        self.x.iter().map(|&xi| f(self, xi)).collect()
    }

    /// A natural cubic spline of `section` over the `x` nodes.
    fn x_spline(&self, section: Vec<Real>) -> QlResult<CubicInterpolation> {
        Ok(CubicNaturalSpline::new(self.x.clone(), section)?.with_extrapolation(true))
    }

    fn check_range(&self, x: Real, y: Real) -> QlResult<()> {
        if x.is_nan() || y.is_nan() {
            fail!("interpolation cannot be evaluated at NaN");
        }
        if !self.allow_extrapolation && !self.is_in_range(x, y) {
            fail!(
                "interpolation range is [{}, {}] x [{}, {}]: extrapolation at ({x}, {y}) not allowed",
                self.x_min(),
                self.x_max(),
                self.y_min(),
                self.y_max()
            );
        }
        Ok(())
    }
}

impl Interpolation2D for BicubicSpline {
    fn value(&self, x: Real, y: Real) -> QlResult<Real> {
        self.check_range(x, y)?;
        self.eval(x, y)
    }

    fn x_min(&self) -> Real {
        self.x[0]
    }

    fn x_max(&self) -> Real {
        self.x[self.x.len() - 1]
    }

    fn y_min(&self) -> Real {
        self.y[0]
    }

    fn y_max(&self) -> Real {
        self.y[self.y.len() - 1]
    }

    fn is_in_range(&self, x: Real, y: Real) -> bool {
        x >= self.x_min() && x <= self.x_max() && y >= self.y_min() && y <= self.y_max()
    }

    fn set_extrapolation(&mut self, allow: bool) {
        self.allow_extrapolation = allow;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(got: Real, expected: Real, tol: Real) {
        assert!(
            (got - expected).abs() <= tol,
            "got {got}, expected {expected}, diff {}",
            (got - expected).abs()
        );
    }

    // Grid of f(x, y) over x = y = i/20, i in 0..100. z[j][i] = f(x[i], y[j]).
    fn grid(f: impl Fn(Real, Real) -> Real) -> (Vec<Real>, Vec<Real>, Vec<Vec<Real>>) {
        let axis: Vec<Real> = (0..100).map(|i| i as Real / 20.0).collect();
        let z: Vec<Vec<Real>> = axis
            .iter()
            .map(|&yj| axis.iter().map(|&xi| f(xi, yj)).collect())
            .collect();
        (axis.clone(), axis, z)
    }

    #[test]
    fn reproduces_analytic_derivatives() {
        // Ported from QuantLib's testBicubicDerivatives: f = y/10 sin(x) + cos(y).
        let f = |x: Real, y: Real| y / 10.0 * x.sin() + y.cos();
        let (x, y, z) = grid(f);
        let bs = BicubicSpline::new(x.clone(), y.clone(), z).unwrap();
        let tol = 0.005;
        for i in (5..95).step_by(10) {
            for j in (5..95).step_by(10) {
                let (px, py) = (x[j], y[i]);
                assert_close(bs.derivative_x(px, py).unwrap(), py / 10.0 * px.cos(), tol);
                assert_close(
                    bs.second_derivative_x(px, py).unwrap(),
                    -py / 10.0 * px.sin(),
                    tol,
                );
                assert_close(
                    bs.derivative_y(px, py).unwrap(),
                    px.sin() / 10.0 - py.sin(),
                    tol,
                );
                assert_close(bs.second_derivative_y(px, py).unwrap(), -py.cos(), tol);
                assert_close(bs.derivative_xy(px, py).unwrap(), px.cos() / 10.0, tol);
            }
        }
    }

    // Small grid of g(x, y) = 1 + 2x + 3y + xy for value-focused checks.
    fn small() -> BicubicSpline {
        let x = vec![0.0, 1.0, 2.0, 3.0];
        let y = vec![0.0, 1.0, 2.0, 4.0];
        let g = |x: Real, y: Real| 1.0 + 2.0 * x + 3.0 * y + x * y;
        let z = y
            .iter()
            .map(|&yj| x.iter().map(|&xi| g(xi, yj)).collect())
            .collect();
        BicubicSpline::new(x, y, z).unwrap()
    }

    #[test]
    fn value_passes_through_nodes() {
        let bs = small();
        let x = [0.0, 1.0, 2.0, 3.0];
        let y = [0.0, 1.0, 2.0, 4.0];
        for &yj in &y {
            for &xi in &x {
                assert_close(
                    bs.value(xi, yj).unwrap(),
                    1.0 + 2.0 * xi + 3.0 * yj + xi * yj,
                    1e-12,
                );
            }
        }
    }

    #[test]
    fn domain_range_and_extrapolation() {
        let bs = small();
        assert_eq!(bs.x_min(), 0.0);
        assert_eq!(bs.x_max(), 3.0);
        assert_eq!(bs.y_min(), 0.0);
        assert_eq!(bs.y_max(), 4.0);
        assert!(bs.is_in_range(1.5, 2.0));
        assert!(!bs.is_in_range(-0.1, 2.0));
        assert!(bs.value(-1.0, 2.0).is_err());
        assert!(bs.value(1.0, 5.0).is_err());

        let ext = small().with_extrapolation(true);
        assert!(ext.allows_extrapolation());
        assert!(ext.value(-1.0, 2.0).is_ok());
        assert!(ext.derivative_x(4.0, 2.0).is_ok());
    }

    #[test]
    fn nan_input_is_rejected() {
        let bs = small().with_extrapolation(true);
        assert!(bs.value(Real::NAN, 1.0).is_err());
        assert!(bs.derivative_y(1.0, Real::NAN).is_err());
    }

    #[test]
    fn factory_builds_and_extrapolation_toggles_through_the_trait() {
        let x = vec![0.0, 1.0, 2.0, 3.0];
        let y = vec![0.0, 1.0, 2.0, 4.0];
        let g = |x: Real, y: Real| 1.0 + 2.0 * x + 3.0 * y;
        let z = y
            .iter()
            .map(|&yj| x.iter().map(|&xi| g(xi, yj)).collect())
            .collect();
        let mut bc = Bicubic.interpolate(x, y, z).unwrap();
        assert!(bc.value(4.0, 1.0).is_err());
        bc.set_extrapolation(true);
        assert_close(bc.value(4.0, 1.0).unwrap(), g(4.0, 1.0), 1e-10);
        bc.set_extrapolation(false);
        assert!(bc.value(4.0, 1.0).is_err());
    }

    #[test]
    fn invalid_grid_rejected() {
        let z = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        // Too few y points.
        assert!(BicubicSpline::new(vec![0.0, 1.0], vec![0.0], vec![vec![1.0, 2.0]]).is_err());
        // Non-increasing y.
        assert!(BicubicSpline::new(vec![0.0, 1.0], vec![1.0, 1.0], z.clone()).is_err());
        // z row count does not match y.
        assert!(BicubicSpline::new(vec![0.0, 1.0], vec![0.0, 1.0], vec![vec![1.0, 2.0]]).is_err());
        // z row width does not match x (surfaced by the row spline).
        assert!(
            BicubicSpline::new(vec![0.0, 1.0], vec![0.0, 1.0], vec![vec![1.0], vec![2.0]]).is_err()
        );
    }
}
