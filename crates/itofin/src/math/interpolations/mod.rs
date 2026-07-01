//! Interpolation framework and concrete interpolations from `ql/math/interpolations/`.

pub mod bilinear;
pub mod flat;
pub mod linear;
pub mod loglinear;

use crate::errors::QlResult;
use crate::types::Real;

/// A one-dimensional interpolation over sorted `x` nodes.
///
/// Mirrors the evaluation surface of QuantLib's `Interpolation`: the value, its
/// derivative, and its antiderivative (`primitive`), each validated against the
/// domain, plus the domain bounds.
pub trait Interpolation {
    /// The interpolated value at `x`. Errors if `x` is outside `[x_min, x_max]`
    /// and extrapolation is disabled.
    fn value(&self, x: Real) -> QlResult<Real>;

    /// The first derivative at `x`.
    fn derivative(&self, x: Real) -> QlResult<Real>;

    /// The antiderivative (integral from `x_min`) at `x`.
    fn primitive(&self, x: Real) -> QlResult<Real>;

    /// The lower end of the interpolation domain.
    fn x_min(&self) -> Real;

    /// The upper end of the interpolation domain.
    fn x_max(&self) -> Real;

    /// Whether `x` lies within `[x_min, x_max]`.
    fn is_in_range(&self, x: Real) -> bool;
}

/// A two-dimensional interpolation over sorted `x` and `y` node grids and a
/// tabulated `z` matrix, where `z[j][i]` is the value at `(x[i], y[j])`.
///
/// Mirrors the evaluation surface of QuantLib's `Interpolation2D`: the value at
/// a point plus the rectangular domain bounds. Like the 1-D
/// [`Interpolation`] trait, `is_in_range` uses plain interval
/// comparisons (QuantLib additionally widens the boundary by `close`).
pub trait Interpolation2D {
    /// The interpolated value at `(x, y)`. Errors if the point is outside the
    /// domain and extrapolation is disabled.
    fn value(&self, x: Real, y: Real) -> QlResult<Real>;

    /// The lower end of the `x` domain.
    fn x_min(&self) -> Real;

    /// The upper end of the `x` domain.
    fn x_max(&self) -> Real;

    /// The lower end of the `y` domain.
    fn y_min(&self) -> Real;

    /// The upper end of the `y` domain.
    fn y_max(&self) -> Real;

    /// Whether `(x, y)` lies within `[x_min, x_max] x [y_min, y_max]`.
    fn is_in_range(&self, x: Real, y: Real) -> bool;
}
