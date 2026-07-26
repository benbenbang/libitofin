//! Flat 2-D extrapolation wrapper over any [`Interpolation2D`].
//!
//! Port of `FlatExtrapolator2D` from `ql/math/interpolations/flatextrapolation2d.hpp`:
//! a decorator that clamps a query `(x, y)` into the decorated interpolation's
//! rectangular domain before delegating, so evaluation outside the grid returns
//! the nearest edge or corner value (flat) rather than extending the boundary
//! surface. The domain bounds and range test pass straight through to the inner
//! interpolation.

use crate::errors::QlResult;
use crate::math::interpolations::Interpolation2D;
use crate::types::Real;

/// Flat-extrapolation decorator over an inner [`Interpolation2D`].
///
/// `value(x, y)` clamps `x` into `[x_min, x_max]` and `y` into `[y_min, y_max]`
/// (mirroring C++'s `bindX`/`bindY`) and evaluates the inner interpolation at the
/// clamped point. Because the clamped point always lands on the closed domain,
/// the inner interpolation is queried in range and its own extrapolation flag is
/// never exercised.
pub struct FlatExtrapolator2D<I: Interpolation2D> {
    inner: I,
}

impl<I: Interpolation2D> FlatExtrapolator2D<I> {
    /// Wraps `inner` so out-of-domain queries clamp to the nearest edge or corner.
    pub fn new(inner: I) -> Self {
        FlatExtrapolator2D { inner }
    }

    /// The wrapped interpolation.
    pub fn inner(&self) -> &I {
        &self.inner
    }

    fn bind_x(&self, x: Real) -> Real {
        x.clamp(self.inner.x_min(), self.inner.x_max())
    }

    fn bind_y(&self, y: Real) -> Real {
        y.clamp(self.inner.y_min(), self.inner.y_max())
    }
}

impl<I: Interpolation2D> Interpolation2D for FlatExtrapolator2D<I> {
    fn value(&self, x: Real, y: Real) -> QlResult<Real> {
        self.inner.value(self.bind_x(x), self.bind_y(y))
    }

    fn x_min(&self) -> Real {
        self.inner.x_min()
    }

    fn x_max(&self) -> Real {
        self.inner.x_max()
    }

    fn y_min(&self) -> Real {
        self.inner.y_min()
    }

    fn y_max(&self) -> Real {
        self.inner.y_max()
    }

    fn is_in_range(&self, x: Real, y: Real) -> bool {
        self.inner.is_in_range(x, y)
    }

    fn set_extrapolation(&mut self, allow: bool) {
        self.inner.set_extrapolation(allow);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::interpolations::bilinear::BilinearInterpolation;

    // The decorated bilinear reproduces f(x, y) = 1 + 2x + 3y + 4xy exactly on a
    // 3x2 grid (x = [0,1,2], y = [0,1]) with a nonzero boundary gradient, so flat
    // clamping and linear extension give genuinely different out-of-grid values.
    fn f(x: Real, y: Real) -> Real {
        1.0 + 2.0 * x + 3.0 * y + 4.0 * x * y
    }

    fn plain() -> BilinearInterpolation {
        let x = vec![0.0, 1.0, 2.0];
        let y = vec![0.0, 1.0];
        let z = y
            .iter()
            .map(|&yj| x.iter().map(|&xi| f(xi, yj)).collect())
            .collect();
        BilinearInterpolation::new(x, y, z).unwrap()
    }

    fn flat() -> FlatExtrapolator2D<BilinearInterpolation> {
        FlatExtrapolator2D::new(plain())
    }

    fn assert_close(got: Real, expected: Real) {
        let tol = 1e-12 * (1.0 + expected.abs());
        assert!(
            (got - expected).abs() <= tol,
            "got {got}, expected {expected}"
        );
    }

    #[test]
    fn in_grid_equals_plain_bilinear() {
        let flat = flat();
        let plain = plain();
        for &(x, y) in &[(0.5, 0.5), (1.5, 0.25), (0.0, 1.0), (2.0, 0.0)] {
            assert_close(flat.value(x, y).unwrap(), plain.value(x, y).unwrap());
        }
    }

    #[test]
    fn clamp_x_only_returns_nearest_edge() {
        let flat = flat();
        let plain = plain().with_extrapolation(true);
        // x past x_max, y in range: clamp x to 2.0, keep y.
        let got = flat.value(3.0, 0.5).unwrap();
        assert_close(got, plain.value(2.0, 0.5).unwrap());
        // Distinct from the plain bilinear's extended surface at the raw point.
        assert!((got - plain.value(3.0, 0.5).unwrap()).abs() > 1e-6);
    }

    #[test]
    fn clamp_y_only_returns_nearest_edge() {
        let flat = flat();
        let plain = plain().with_extrapolation(true);
        // y past y_max, x in range: clamp y to 1.0, keep x.
        let got = flat.value(0.5, 5.0).unwrap();
        assert_close(got, plain.value(0.5, 1.0).unwrap());
        assert!((got - plain.value(0.5, 5.0).unwrap()).abs() > 1e-6);
    }

    #[test]
    fn clamp_both_returns_corner_node() {
        let flat = flat();
        let plain = plain().with_extrapolation(true);
        // Both axes past their max: clamp to the (x_max, y_max) corner node.
        let got = flat.value(3.0, 5.0).unwrap();
        assert_close(got, f(2.0, 1.0));
        assert_close(got, plain.value(2.0, 1.0).unwrap());
        assert!((got - plain.value(3.0, 5.0).unwrap()).abs() > 1e-6);
        // Lower corner too.
        let low = flat.value(-1.0, -1.0).unwrap();
        assert_close(low, f(0.0, 0.0));
    }

    #[test]
    fn bounds_pass_through() {
        let flat = flat();
        assert_eq!(flat.x_min(), 0.0);
        assert_eq!(flat.x_max(), 2.0);
        assert_eq!(flat.y_min(), 0.0);
        assert_eq!(flat.y_max(), 1.0);
        assert!(flat.is_in_range(1.0, 0.5));
        assert!(!flat.is_in_range(3.0, 0.5));
        assert!(!flat.is_in_range(1.0, 2.0));
    }

    #[test]
    fn nan_input_is_rejected() {
        let flat = flat();
        assert!(flat.value(Real::NAN, 0.5).is_err());
        assert!(flat.value(0.5, Real::NAN).is_err());
    }
}
