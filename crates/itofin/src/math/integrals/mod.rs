//! Numerical integration ported from `ql/math/integrals/`.

pub mod discrete;
pub mod kronrod;
pub mod lobatto;
pub mod piecewise;
pub mod segment;
pub mod simpson;
pub mod tabulatedgausslegendre;
pub mod trapezoid;
pub mod twodimensional;

use crate::errors::QlResult;
use crate::fail;
use crate::types::Real;

/// Validates the absolute accuracy shared by the adaptive integrators: finite
/// and above machine epsilon, matching QuantLib's `Integrator` precondition
/// (extended to reject the non-finite values QuantLib leaves unchecked).
pub(crate) fn require_accuracy(accuracy: Real) -> QlResult<()> {
    if !accuracy.is_finite() || accuracy <= Real::EPSILON {
        fail!("required accuracy ({accuracy}) must be finite and exceed machine epsilon");
    }
    Ok(())
}

/// A one-dimensional numerical integrator over `[a, b]`.
///
/// Mirrors QuantLib's `Integrator`, but functional: `integrate` returns
/// `Ok(value)` when the quadrature meets its contract and `Err` when it cannot
/// (non-convergence or an exhausted evaluation budget), instead of QuantLib's
/// post-hoc `integrationSuccess()` state. Concrete integrators own their own
/// configuration; the trait itself is only the integration contract, so it is
/// not object-safe (the methods are generic over the integrand).
pub trait Integrator {
    /// Integrates `f` over `[a, b]` for `a < b`; the [`Integrator::integrate`]
    /// driver guarantees ordered, non-degenerate limits.
    fn integrate_impl<F>(&self, f: &mut F, a: Real, b: Real) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real;

    /// Integrates `f` over `[a, b]`, handling degenerate (`a == b`) and reversed
    /// (`b < a`) limits before delegating to `integrate_impl`.
    ///
    /// # Errors
    ///
    /// Returns an error if either bound is not finite. This trait integrates
    /// over finite intervals only; infinite-domain (improper) integration is the
    /// job of a separate, dedicated integrator, not of this base.
    #[allow(clippy::float_cmp)]
    fn integrate<F>(&self, mut f: F, a: Real, b: Real) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        // A non-finite bound would slip past both the `a == b` and `b > a`
        // checks (NaN compares false) and yield a silent NaN, so reject it up
        // front rather than integrate a garbage interval.
        if !a.is_finite() || !b.is_finite() {
            fail!("integration bounds must be finite, got [{a}, {b}]");
        }
        // Mirrors QuantLib's Integrator::operator(): an empty interval
        // integrates to zero, and reversed limits negate the result.
        if a == b {
            return Ok(0.0);
        }
        if b > a {
            self.integrate_impl(&mut f, a, b)
        } else {
            Ok(-self.integrate_impl(&mut f, b, a)?)
        }
    }
}
