//! Bilinear interpolation over a 2-D grid.
//!
//! Port of `BilinearInterpolation` from
//! `ql/math/interpolations/bilinearinterpolation.hpp`: within each grid cell the
//! value is the tensor-product of two linear interpolations. It reproduces any
//! `a + b*x + c*y + d*x*y` function exactly. `locate` clamps to the end cells so
//! enabling extrapolation extends the boundary cells' bilinear surfaces.

use crate::errors::QlResult;
use crate::fail;
use crate::math::interpolations::{Interpolation2D, Interpolator2D};
use crate::types::{Real, Size};

/// Factory for [`BilinearInterpolation`] (QuantLib's `Bilinear` traits
/// class).
#[derive(Clone, Copy, Default)]
pub struct Bilinear;

impl Interpolator2D for Bilinear {
    type Output = BilinearInterpolation;

    fn interpolate(
        &self,
        x: Vec<Real>,
        y: Vec<Real>,
        z: Vec<Vec<Real>>,
    ) -> QlResult<BilinearInterpolation> {
        BilinearInterpolation::new(x, y, z)
    }
}

/// Bilinear interpolation over strictly increasing `x` and `y` node grids.
///
/// `z[j][i]` holds the tabulated value at `(x[i], y[j])`: the outer index runs
/// over `y` (rows), the inner over `x` (columns).
pub struct BilinearInterpolation {
    x: Vec<Real>,
    y: Vec<Real>,
    z: Vec<Vec<Real>>,
    allow_extrapolation: bool,
}

impl BilinearInterpolation {
    /// Builds an interpolation over the grid `(x, y)` with values `z`. Both axes
    /// must be strictly increasing with at least two points, and `z` must be a
    /// `y.len()` by `x.len()` matrix of finite values.
    pub fn new(x: Vec<Real>, y: Vec<Real>, z: Vec<Vec<Real>>) -> QlResult<Self> {
        validate_axis(&x, "x")?;
        validate_axis(&y, "y")?;
        if z.len() != y.len() {
            fail!(
                "z must have one row per y node ({} rows vs {} y values)",
                z.len(),
                y.len()
            );
        }
        for (j, row) in z.iter().enumerate() {
            if row.len() != x.len() {
                fail!(
                    "z row {j} must have one column per x node ({} vs {})",
                    row.len(),
                    x.len()
                );
            }
            for (i, &zji) in row.iter().enumerate() {
                if !zji.is_finite() {
                    fail!("z values must be finite, got z[{j}][{i}] = {zji}");
                }
            }
        }
        Ok(BilinearInterpolation {
            x,
            y,
            z,
            allow_extrapolation: false,
        })
    }

    /// Sets whether evaluation outside the domain is permitted (extending the
    /// boundary cells) rather than an error.
    pub fn with_extrapolation(mut self, allow: bool) -> Self {
        self.allow_extrapolation = allow;
        self
    }

    /// Whether extrapolation is currently permitted.
    pub fn allows_extrapolation(&self) -> bool {
        self.allow_extrapolation
    }

    /// The index of the cell containing `v`, clamped to the end cells.
    fn locate(nodes: &[Real], v: Real) -> Size {
        let n = nodes.len();
        if v < nodes[0] {
            0
        } else if v > nodes[n - 1] {
            n - 2
        } else {
            nodes[..n - 1].partition_point(|&ni| ni <= v) - 1
        }
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

fn validate_axis(vals: &[Real], name: &str) -> QlResult<()> {
    if vals.len() < 2 {
        fail!(
            "bilinear interpolation needs at least 2 {name} points, got {}",
            vals.len()
        );
    }
    for &v in vals {
        if !v.is_finite() {
            fail!("{name} values must be finite, got {v}");
        }
    }
    for w in vals.windows(2) {
        if w[1] <= w[0] {
            fail!("{name} values must be strictly increasing");
        }
    }
    Ok(())
}

impl Interpolation2D for BilinearInterpolation {
    fn value(&self, x: Real, y: Real) -> QlResult<Real> {
        self.check_range(x, y)?;
        let i = Self::locate(&self.x, x);
        let j = Self::locate(&self.y, y);
        let z1 = self.z[j][i];
        let z2 = self.z[j][i + 1];
        let z3 = self.z[j + 1][i];
        let z4 = self.z[j + 1][i + 1];
        let t = (x - self.x[i]) / (self.x[i + 1] - self.x[i]);
        let u = (y - self.y[j]) / (self.y[j + 1] - self.y[j]);
        Ok((1.0 - t) * (1.0 - u) * z1 + t * (1.0 - u) * z2 + (1.0 - t) * u * z3 + t * u * z4)
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

    // Bilinear reproduces f(x, y) = 1 + 2x + 3y + 4xy exactly. Grid x = [0,1,2],
    // y = [0,1,3]; z[j][i] = f(x[i], y[j]).
    fn f(x: Real, y: Real) -> Real {
        1.0 + 2.0 * x + 3.0 * y + 4.0 * x * y
    }

    fn sample() -> BilinearInterpolation {
        let x = vec![0.0, 1.0, 2.0];
        let y = vec![0.0, 1.0, 3.0];
        let z = y
            .iter()
            .map(|&yj| x.iter().map(|&xi| f(xi, yj)).collect())
            .collect();
        BilinearInterpolation::new(x, y, z).unwrap()
    }

    fn assert_close(got: Real, expected: Real) {
        let tol = 1e-12 * (1.0 + expected.abs());
        assert!(
            (got - expected).abs() <= tol,
            "got {got}, expected {expected}"
        );
    }

    #[test]
    fn value_at_nodes_returns_z() {
        let bl = sample();
        for &x in &[0.0, 1.0, 2.0] {
            for &y in &[0.0, 1.0, 3.0] {
                assert_close(bl.value(x, y).unwrap(), f(x, y));
            }
        }
    }

    #[test]
    fn reproduces_bilinear_function_in_interior() {
        let bl = sample();
        for &(x, y) in &[(0.5, 0.5), (1.5, 2.0), (0.25, 1.7), (1.9, 0.1)] {
            assert_close(bl.value(x, y).unwrap(), f(x, y));
        }
    }

    #[test]
    fn cell_center_is_corner_average() {
        // Cell [0,1] x [0,1] corners: f(0,0)=1, f(1,0)=3, f(0,1)=4, f(1,1)=10.
        let bl = sample();
        assert_close(bl.value(0.5, 0.5).unwrap(), (1.0 + 3.0 + 4.0 + 10.0) / 4.0);
    }

    #[test]
    fn edge_reduces_to_linear() {
        // Along y = 0 the value is linear in x between f(0,0)=1 and f(1,0)=3.
        let bl = sample();
        assert_close(bl.value(0.5, 0.0).unwrap(), 2.0);
    }

    #[test]
    fn domain_and_in_range() {
        let bl = sample();
        assert_eq!(bl.x_min(), 0.0);
        assert_eq!(bl.x_max(), 2.0);
        assert_eq!(bl.y_min(), 0.0);
        assert_eq!(bl.y_max(), 3.0);
        assert!(bl.is_in_range(1.0, 2.0));
        assert!(!bl.is_in_range(-0.1, 1.0));
        assert!(!bl.is_in_range(1.0, 3.1));
    }

    #[test]
    fn extrapolation_disabled_errors_out_of_range() {
        let bl = sample();
        assert!(bl.value(-1.0, 1.0).is_err());
        assert!(bl.value(1.0, 4.0).is_err());
    }

    #[test]
    fn extrapolation_enabled_extends_boundary_cell() {
        // The globally bilinear f is reproduced outside the grid too.
        let bl = sample().with_extrapolation(true);
        assert!(bl.allows_extrapolation());
        assert_close(bl.value(-0.5, -0.5).unwrap(), f(-0.5, -0.5));
        assert_close(bl.value(2.5, 3.5).unwrap(), f(2.5, 3.5));
    }

    #[test]
    fn factory_builds_and_extrapolation_toggles_through_the_trait() {
        let mut bl = Bilinear
            .interpolate(vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 3.0], {
                let x = [0.0, 1.0, 2.0];
                [0.0, 1.0, 3.0]
                    .iter()
                    .map(|&yj| x.iter().map(|&xi| f(xi, yj)).collect())
                    .collect()
            })
            .unwrap();
        assert!(bl.value(-0.5, 1.0).is_err());
        bl.set_extrapolation(true);
        assert_close(bl.value(-0.5, -0.5).unwrap(), f(-0.5, -0.5));
        bl.set_extrapolation(false);
        assert!(bl.value(-0.5, 1.0).is_err());
    }

    #[test]
    fn nan_input_is_rejected() {
        let bl = sample().with_extrapolation(true);
        assert!(bl.value(Real::NAN, 1.0).is_err());
        assert!(bl.value(1.0, Real::NAN).is_err());
    }

    #[test]
    fn invalid_grid_rejected() {
        let good_z = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        // Too few points on an axis.
        assert!(BilinearInterpolation::new(vec![0.0], vec![0.0, 1.0], vec![vec![1.0]]).is_err());
        // Non-increasing axis.
        assert!(
            BilinearInterpolation::new(vec![1.0, 1.0], vec![0.0, 1.0], good_z.clone()).is_err()
        );
        // z row count does not match y.
        assert!(
            BilinearInterpolation::new(vec![0.0, 1.0], vec![0.0, 1.0], vec![vec![1.0, 2.0]])
                .is_err()
        );
        // z column count does not match x.
        assert!(
            BilinearInterpolation::new(vec![0.0, 1.0], vec![0.0, 1.0], vec![vec![1.0], vec![2.0]])
                .is_err()
        );
        // Non-finite z.
        assert!(
            BilinearInterpolation::new(
                vec![0.0, 1.0],
                vec![0.0, 1.0],
                vec![vec![1.0, Real::NAN], vec![3.0, 4.0]]
            )
            .is_err()
        );
    }
}
