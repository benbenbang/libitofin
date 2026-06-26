//! Interpolation framework and concrete interpolations from `ql/math/interpolations/`.

pub mod linear;

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
