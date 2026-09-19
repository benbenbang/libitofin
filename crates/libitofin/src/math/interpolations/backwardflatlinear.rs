//! Backward-flat in `x`, linear in `y` interpolation over a 2-D grid.
//!
//! Port of `BackwardflatLinearInterpolation` from
//! `ql/math/interpolations/backwardflatlinearinterpolation.hpp`. Along `x` the
//! value is piecewise constant, taking the column of the NEXT node: `x <= x[0]`
//! reads column 0, `x == x[i]` reads column `i`, and `x` strictly inside
//! `(x[i], x[i+1])` reads column `i+1` (hpp:57-70). Along `y` it is linear
//! between the two bracketing rows. As in [`bilinear`](super::bilinear),
//! `locate` clamps to the end cells so enabling extrapolation extends the
//! boundary rows linearly in `y` and the boundary columns flat in `x`.

use crate::errors::QlResult;
use crate::fail;
use crate::math::interpolations::{Interpolation2D, Interpolator2D};
use crate::types::{Real, Size};

/// Factory for [`BackwardflatLinearInterpolation`] (QuantLib's
/// `BackwardflatLinear` traits class).
#[derive(Clone, Copy, Default)]
pub struct BackwardflatLinear;

impl Interpolator2D for BackwardflatLinear {
    type Output = BackwardflatLinearInterpolation;

    fn interpolate(
        &self,
        x: Vec<Real>,
        y: Vec<Real>,
        z: Vec<Vec<Real>>,
    ) -> QlResult<BackwardflatLinearInterpolation> {
        BackwardflatLinearInterpolation::new(x, y, z)
    }
}

/// Backward-flat (in `x`) linear (in `y`) interpolation over strictly
/// increasing `x` and `y` node grids.
///
/// `z[j][i]` holds the tabulated value at `(x[i], y[j])`: the outer index runs
/// over `y` (rows), the inner over `x` (columns).
pub struct BackwardflatLinearInterpolation {
    x: Vec<Real>,
    y: Vec<Real>,
    z: Vec<Vec<Real>>,
    allow_extrapolation: bool,
}

impl BackwardflatLinearInterpolation {
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
        Ok(BackwardflatLinearInterpolation {
            x,
            y,
            z,
            allow_extrapolation: false,
        })
    }

    /// Sets whether evaluation outside the domain is permitted rather than an
    /// error.
    pub fn with_extrapolation(mut self, allow: bool) -> Self {
        self.allow_extrapolation = allow;
        self
    }

    /// Whether extrapolation is currently permitted.
    pub fn allows_extrapolation(&self) -> bool {
        self.allow_extrapolation
    }

    /// The index of the cell containing `v`, clamped to the end cells
    /// (`Interpolation2D::templateImpl::locateX/locateY`).
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

    /// The `z` column backward-flat `x` reads: the node at or after `x`,
    /// clamped to the first column (hpp:59-69, exact node comparison as C++).
    #[allow(clippy::float_cmp)]
    fn column(&self, x: Real) -> Size {
        if x <= self.x[0] {
            return 0;
        }
        let i = Self::locate(&self.x, x);
        if x == self.x[i] { i } else { i + 1 }
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
            "backward-flat linear interpolation needs at least 2 {name} points, got {}",
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

impl Interpolation2D for BackwardflatLinearInterpolation {
    fn value(&self, x: Real, y: Real) -> QlResult<Real> {
        self.check_range(x, y)?;
        let col = self.column(x);
        let j = Self::locate(&self.y, y);
        let z1 = self.z[j][col];
        let z2 = self.z[j + 1][col];
        let u = (y - self.y[j]) / (self.y[j + 1] - self.y[j]);
        Ok((1.0 - u) * z1 + u * z2)
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

    // Grid x = [0, 1, 2], y = [0, 1]; z[j][i] = 10*i + j, so every node is
    // distinct and the column (x node) and row (y node) read off the value.
    fn sample() -> BackwardflatLinearInterpolation {
        BackwardflatLinearInterpolation::new(
            vec![0.0, 1.0, 2.0],
            vec![0.0, 1.0],
            vec![vec![0.0, 10.0, 20.0], vec![1.0, 11.0, 21.0]],
        )
        .unwrap()
    }

    fn assert_close(got: Real, expected: Real) {
        assert!(
            (got - expected).abs() <= 1e-12 * (1.0 + expected.abs()),
            "got {got}, expected {expected}"
        );
    }

    #[test]
    fn value_at_nodes_returns_z() {
        let bf = sample();
        for (i, &x) in [0.0, 1.0, 2.0].iter().enumerate() {
            for (j, &y) in [0.0, 1.0].iter().enumerate() {
                assert_close(bf.value(x, y).unwrap(), 10.0 * i as Real + j as Real);
            }
        }
    }

    #[test]
    fn x_between_nodes_reads_the_next_column() {
        let bf = sample();
        // Hand-derived from hpp:59-69: locateX(0.5) = 0, 0.5 != x[0], column 1.
        assert_close(bf.value(0.5, 0.0).unwrap(), 10.0);
        // locateX(1.5) = 1, 1.5 != x[1], column 2.
        assert_close(bf.value(1.5, 0.0).unwrap(), 20.0);
        // Just past a node still reads the following column, not the node's.
        assert_close(bf.value(1.0 + 1e-9, 0.0).unwrap(), 20.0);
    }

    #[test]
    fn x_at_or_below_first_node_reads_column_zero() {
        let bf = sample().with_extrapolation(true);
        assert_close(bf.value(0.0, 0.0).unwrap(), 0.0);
        assert_close(bf.value(-3.0, 0.0).unwrap(), 0.0);
        // Past the last node (extrapolating) the last column is read flat.
        assert_close(bf.value(5.0, 0.0).unwrap(), 20.0);
    }

    #[test]
    fn y_interior_is_linear_between_rows() {
        let bf = sample();
        // Column 1 (x = 1): rows 10 and 11, u = 0.25 -> 10.25.
        assert_close(bf.value(1.0, 0.25).unwrap(), 10.25);
        // Column 2 via x = 1.5: rows 20 and 21, u = 0.5 -> 20.5.
        assert_close(bf.value(1.5, 0.5).unwrap(), 20.5);
        // Linear extrapolation in y past the last row: u = 2 -> 0 + 2*(1 - 0).
        let bf = bf.with_extrapolation(true);
        assert_close(bf.value(0.0, 2.0).unwrap(), 2.0);
    }

    #[test]
    fn domain_range_and_extrapolation_flag() {
        let mut bf = sample();
        assert_eq!(bf.x_min(), 0.0);
        assert_eq!(bf.x_max(), 2.0);
        assert_eq!(bf.y_min(), 0.0);
        assert_eq!(bf.y_max(), 1.0);
        assert!(bf.is_in_range(1.0, 0.5));
        assert!(!bf.is_in_range(-0.1, 0.5));
        assert!(!bf.is_in_range(1.0, 1.1));
        assert!(bf.value(-1.0, 0.5).is_err());
        assert!(bf.value(1.0, 2.0).is_err());
        bf.set_extrapolation(true);
        assert!(bf.allows_extrapolation());
        assert!(bf.value(-1.0, 0.5).is_ok());
        assert!(bf.value(Real::NAN, 0.5).is_err());
    }

    #[test]
    fn factory_and_invalid_grids() {
        let z = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        let bf = BackwardflatLinear
            .interpolate(vec![0.0, 1.0], vec![0.0, 1.0], z.clone())
            .unwrap();
        assert_close(bf.value(0.5, 0.5).unwrap(), 3.0);
        let bad = [
            BackwardflatLinearInterpolation::new(vec![0.0], vec![0.0, 1.0], vec![vec![1.0]]),
            BackwardflatLinearInterpolation::new(vec![1.0, 1.0], vec![0.0, 1.0], z.clone()),
            BackwardflatLinearInterpolation::new(
                vec![0.0, 1.0],
                vec![0.0, 1.0],
                vec![z[0].clone()],
            ),
            BackwardflatLinearInterpolation::new(
                vec![0.0, 1.0],
                vec![0.0, 1.0],
                vec![vec![1.0], vec![2.0]],
            ),
            BackwardflatLinearInterpolation::new(
                vec![0.0, 1.0],
                vec![0.0, 1.0],
                vec![vec![1.0, Real::NAN], vec![3.0, 4.0]],
            ),
        ];
        assert!(bad.iter().all(|r| r.is_err()));
    }
}
