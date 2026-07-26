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
