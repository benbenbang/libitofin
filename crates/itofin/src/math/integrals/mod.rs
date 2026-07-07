//! Numerical integration ported from `ql/math/integrals/`.

pub mod discrete;
pub mod exponential_integrals;
pub mod expsinh;
pub mod filon;
pub mod gaussiannoncentralchisquaredpolynomial;
pub mod gaussianorthogonalpolynomial;
pub mod gaussianquadratures;
pub mod gausslaguerrecosinepolynomial;
pub mod kronrod;
pub mod lobatto;
pub mod momentbasedgaussianpolynomial;
pub mod piecewise;
pub mod segment;
pub mod simpson;
pub mod tabulatedgausslegendre;
pub mod tanhsinh;
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

/// Trapezoidal double-exponential quadrature with successive level refinement,
/// shared by the tanh-sinh and exp-sinh integrators.
///
/// Sums `w * f(x)` over the grid `t = k * h` for `|t| <= t_max`, where `node`
/// maps `t` to the transformed abscissa and weight. `None` drops a node past
/// the transform's usable floating-point range and ends that tail, so `node`'s
/// truncation must be monotone in `|t|` on each side, as it is for both
/// transforms. Each refinement halves `h`, reusing every previous sample.
///
/// Two rules are taken from `boost::math::quadrature`'s DE schemes. A tail
/// walk stops, for this and all finer levels, once a term no longer changes
/// the running (nonzero) sum, so the integrand is never evaluated at extreme
/// abscissas where a decayed-to-zero product could overflow into `inf * 0`;
/// while the sum is still zero the full range is kept, in case the mass has
/// simply not been found yet. And iteration stops when two consecutive levels
/// agree to `rel_tolerance` against the L1 norm of the integral, checked from
/// the second refinement onward.
pub(crate) fn de_quadrature<F, N>(
    f: &mut F,
    node: N,
    t_max: Real,
    rel_tolerance: Real,
    max_refinements: usize,
) -> QlResult<Real>
where
    F: FnMut(Real) -> Real,
    N: Fn(Real) -> Option<(Real, Real)>,
{
    struct Tally {
        sum: Real,
        abs_sum: Real,
    }

    fn term<F, N>(f: &mut F, node: &N, t: Real) -> QlResult<Option<(Real, Real)>>
    where
        F: FnMut(Real) -> Real,
        N: Fn(Real) -> Option<(Real, Real)>,
    {
        let Some((x, w)) = node(t) else {
            return Ok(None);
        };
        let y = f(x);
        if !y.is_finite() {
            fail!("integrand returned a non-finite value ({y}) at x = {x}");
        }
        Ok(Some((w * y, (w * y).abs())))
    }

    fn walk_side<F, N>(
        f: &mut F,
        node: &N,
        start: Real,
        step: Real,
        cutoff: &mut Real,
        tally: &mut Tally,
    ) -> QlResult<()>
    where
        F: FnMut(Real) -> Real,
        N: Fn(Real) -> Option<(Real, Real)>,
    {
        let mut t = start;
        while t.abs() <= *cutoff {
            let Some((v, av)) = term(f, node, t)? else {
                break;
            };
            let before = tally.sum;
            tally.sum += v;
            tally.abs_sum += av;
            if tally.sum == before && tally.sum != 0.0 {
                *cutoff = t.abs();
                break;
            }
            t += step;
        }
        Ok(())
    }

    let mut tally = Tally {
        sum: 0.0,
        abs_sum: 0.0,
    };
    if let Some((v, av)) = term(f, &node, 0.0)? {
        tally.sum += v;
        tally.abs_sum += av;
    }
    let mut pos_cutoff = t_max;
    let mut neg_cutoff = t_max;
    walk_side(f, &node, 1.0, 1.0, &mut pos_cutoff, &mut tally)?;
    walk_side(f, &node, -1.0, -1.0, &mut neg_cutoff, &mut tally)?;
    let mut h = 1.0;
    let mut value = tally.sum;
    for refinement in 0..max_refinements {
        h *= 0.5;
        walk_side(f, &node, h, 2.0 * h, &mut pos_cutoff, &mut tally)?;
        walk_side(f, &node, -h, -2.0 * h, &mut neg_cutoff, &mut tally)?;
        let refined = h * tally.sum;
        if !refined.is_finite() {
            fail!(
                "double-exponential quadrature overflowed accumulating an integral near the top of the f64 range"
            );
        }
        let error = (refined - value).abs();
        value = refined;
        if refinement > 0 && error <= rel_tolerance * (h * tally.abs_sum) {
            return Ok(value);
        }
    }
    fail!(
        "double-exponential quadrature failed to reach relative tolerance {rel_tolerance} within {max_refinements} refinements"
    );
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
