//! Discrete integration over sampled data and fixed grids.
//!
//! Port of `ql/math/integrals/discreteintegrals.{hpp,cpp}`:
//!
//! * [`DiscreteTrapezoidIntegral`] and [`DiscreteSimpsonIntegral`] integrate a
//!   function already sampled at arbitrary abscissae `(x, f)` - no callback.
//! * [`DiscreteTrapezoidIntegrator`] and [`DiscreteSimpsonIntegrator`] are
//!   [`Integrator`]s that sample a callback on a fixed equally-spaced grid.

use crate::errors::QlResult;
use crate::math::integrals::Integrator;
use crate::types::{Real, Size};
use crate::{fail, require};

/// Reject non-finite abscissae or ordinates before they enter the quadrature.
///
/// Divergence: `discreteintegrals.cpp` guards only `n == x.size()`
/// (`:28`, `:46`). A NaN or infinite sample propagates through the weighted sum
/// and the caller receives a NaN integral with no indication of which sample
/// produced it.
fn require_finite_samples(x: &[Real], f: &[Real]) -> QlResult<()> {
    for &xi in x {
        if !xi.is_finite() {
            fail!("x values must be finite, got {xi}");
        }
    }
    for &fi in f {
        if !fi.is_finite() {
            fail!("function values must be finite, got {fi}");
        }
    }
    Ok(())
}

/// Trapezoidal integration of samples `(x, f)` on an arbitrary grid.
#[derive(Clone, Copy, Debug, Default)]
pub struct DiscreteTrapezoidIntegral;

impl DiscreteTrapezoidIntegral {
    /// Integrates the samples `f` taken at abscissae `x` with the trapezoid
    /// rule. Fewer than two points integrate to zero.
    ///
    /// # Errors
    ///
    /// Returns an error if `x` and `f` have different lengths, or if any sample
    /// is non-finite.
    pub fn integrate(&self, x: &[Real], f: &[Real]) -> QlResult<Real> {
        require!(
            x.len() == f.len(),
            "inconsistent size: x has {}, f has {}",
            x.len(),
            f.len()
        );
        require_finite_samples(x, f)?;
        let n = f.len();
        if n < 2 {
            return Ok(0.0);
        }
        let mut sum = 0.0;
        for i in 0..n - 1 {
            sum += (x[i + 1] - x[i]) * (f[i] + f[i + 1]);
        }
        Ok(0.5 * sum)
    }
}

/// Simpson integration of samples `(x, f)` on an arbitrary grid.
#[derive(Clone, Copy, Debug, Default)]
pub struct DiscreteSimpsonIntegral;

impl DiscreteSimpsonIntegral {
    /// Integrates the samples `f` taken at abscissae `x` with a Simpson rule
    /// generalised to non-uniform spacing, closing an odd final panel with the
    /// trapezoid rule. Fewer than two points integrate to zero.
    ///
    /// Divergence: the distinct-adjacent-`x` guard has no counterpart in
    /// `discreteintegrals.cpp`. The panel weight is `dd / (6 * dxjp1 * dxj)`,
    /// so a repeated abscissa divides by zero and C++ returns NaN silently.
    ///
    /// # Errors
    ///
    /// Returns an error if `x` and `f` have different lengths, if any sample is
    /// non-finite, or if two adjacent abscissae coincide.
    pub fn integrate(&self, x: &[Real], f: &[Real]) -> QlResult<Real> {
        require!(
            x.len() == f.len(),
            "inconsistent size: x has {}, f has {}",
            x.len(),
            f.len()
        );
        require_finite_samples(x, f)?;
        let n = f.len();
        if n < 2 {
            return Ok(0.0);
        }
        let mut sum = 0.0;
        let mut j = 0;
        while j + 2 < n {
            let dxj = x[j + 1] - x[j];
            let dxjp1 = x[j + 2] - x[j + 1];
            if dxj == 0.0 || dxjp1 == 0.0 {
                fail!("adjacent x values must be distinct");
            }

            let alpha = dxjp1 * (2.0 * dxj - dxjp1);
            let dd = dxj + dxjp1;
            let k = dd / (6.0 * dxjp1 * dxj);
            let beta = dd * dd;
            let gamma = dxj * (2.0 * dxjp1 - dxj);

            sum += k * (alpha * f[j] + beta * f[j + 1] + gamma * f[j + 2]);
            j += 2;
        }
        // An even number of points leaves a final panel with no Simpson triple;
        // close it with the trapezoid rule.
        if n.is_multiple_of(2) {
            sum += 0.5 * (x[n - 1] - x[n - 2]) * (f[n - 1] + f[n - 2]);
        }
        Ok(sum)
    }
}

/// Rejects an evaluation count below the `minimum` a rule needs.
fn require_grid(evaluations: Size, minimum: Size) -> QlResult<()> {
    if evaluations < minimum {
        fail!("at least {minimum} evaluations needed, got {evaluations}");
    }
    Ok(())
}

/// Fixed-grid trapezoidal [`Integrator`]: samples the integrand at
/// `evaluations` equally-spaced nodes over `[a, b]`.
#[derive(Clone, Copy, Debug)]
pub struct DiscreteTrapezoidIntegrator {
    evaluations: Size,
}

impl DiscreteTrapezoidIntegrator {
    /// A trapezoid integrator sampling `evaluations` (at least 2) grid nodes.
    ///
    /// # Errors
    ///
    /// Returns an error if `evaluations < 2`.
    pub fn new(evaluations: Size) -> QlResult<Self> {
        require_grid(evaluations, 2)?;
        Ok(DiscreteTrapezoidIntegrator { evaluations })
    }
}

impl Integrator for DiscreteTrapezoidIntegrator {
    fn integrate_impl<F>(&self, f: &mut F, a: Real, b: Real) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        let n = self.evaluations - 1;
        let d = (b - a) / n as Real;
        let mut x = a;
        let mut sum = f(a) * 0.5;
        for _ in 0..n - 1 {
            x += d;
            sum += f(x);
        }
        sum += f(b) * 0.5;
        Ok(d * sum)
    }
}

/// Fixed-grid Simpson [`Integrator`]: samples the integrand at `evaluations`
/// equally-spaced nodes over `[a, b]`.
#[derive(Clone, Copy, Debug)]
pub struct DiscreteSimpsonIntegrator {
    evaluations: Size,
}

impl DiscreteSimpsonIntegrator {
    /// A Simpson integrator sampling `evaluations` (at least 3) grid nodes.
    ///
    /// Simpson needs at least two intervals: with a single interval
    /// (`evaluations == 2`) the tail branch double-counts `f(a)` and returns a
    /// wrong result, so require three nodes.
    ///
    /// # Errors
    ///
    /// Returns an error if `evaluations < 3`.
    pub fn new(evaluations: Size) -> QlResult<Self> {
        require_grid(evaluations, 3)?;
        Ok(DiscreteSimpsonIntegrator { evaluations })
    }
}

impl Integrator for DiscreteSimpsonIntegrator {
    fn integrate_impl<F>(&self, f: &mut F, a: Real, b: Real) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        let n = self.evaluations - 1;
        let d = (b - a) / n as Real;
        let d2 = d * 2.0;

        // Odd interior nodes (weight 4), doubled here and again below.
        let mut sum = 0.0;
        let mut x = a + d;
        let mut i = 1;
        while i < n {
            sum += f(x);
            x += d2;
            i += 2;
        }
        sum *= 2.0;

        // Even interior nodes (weight 2).
        x = a + d2;
        let mut i = 2;
        while i + 1 < n {
            sum += f(x);
            x += d2;
            i += 2;
        }
        sum *= 2.0;

        sum += f(a);
        // An odd node count leaves a trailing panel closed by a 3/8-style tail.
        if !n.is_multiple_of(2) {
            sum += 1.5 * f(b) + 2.5 * f(b - d);
        } else {
            sum += f(b);
        }
        Ok(d / 3.0 * sum)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::distributions::normal::NormalDistribution;

    // Test functions from QuantLib's integrals suite.
    fn f1(x: Real) -> Real {
        1.2 * x * x + 3.2 * x + 3.1
    }
    fn f2(x: Real) -> Real {
        4.3 * (x - 2.34) * (x - 2.34) - 6.2 * (x - 2.34) + f1(2.34)
    }

    // Port of testDiscreteIntegrals: sampled Simpson and trapezoid on a
    // non-uniform grid, against the hand-computed expected sums.
    #[test]
    fn sampled_formulae_match_expected_sums() {
        let x = [1.0, 2.02, 2.34, 3.3, 4.2, 4.6];
        let f = [f1(1.0), f1(2.02), f1(2.34), f2(3.3), f2(4.2), f2(4.6)];

        let expected_simpson = 16.0401216 + 30.4137528 + 0.2 * f2(4.2) + 0.2 * f2(4.6);
        let expected_trapezoid = 0.5 * (f1(1.0) + f1(2.02)) * 1.02
            + 0.5 * (f1(2.02) + f1(2.34)) * 0.32
            + 0.5 * (f2(2.34) + f2(3.3)) * 0.96
            + 0.5 * (f2(3.3) + f2(4.2)) * 0.9
            + 0.5 * (f2(4.2) + f2(4.6)) * 0.4;

        let tol = 1e-12;
        let simpson = DiscreteSimpsonIntegral.integrate(&x, &f).unwrap();
        let trapezoid = DiscreteTrapezoidIntegral.integrate(&x, &f).unwrap();
        assert!(
            (simpson - expected_simpson).abs() < tol,
            "simpson {simpson} vs {expected_simpson}"
        );
        assert!(
            (trapezoid - expected_trapezoid).abs() < tol,
            "trapezoid {trapezoid} vs {expected_trapezoid}"
        );
    }

    // Port of testDiscreteIntegralsWithFewPoints: on a degenerate grid (0 or 1
    // points) both rules integrate to zero without indexing out of bounds.
    #[test]
    fn degenerate_grids_integrate_to_zero() {
        for n in 0..2 {
            let x: Vec<Real> = (0..n).map(|i| i as Real).collect();
            let f = vec![1.0; n];
            assert_eq!(DiscreteTrapezoidIntegral.integrate(&x, &f).unwrap(), 0.0);
            assert_eq!(DiscreteSimpsonIntegral.integrate(&x, &f).unwrap(), 0.0);
        }
    }

    #[test]
    fn sampled_formulae_reject_mismatched_lengths() {
        assert!(
            DiscreteTrapezoidIntegral
                .integrate(&[1.0, 2.0], &[1.0])
                .is_err()
        );
        assert!(
            DiscreteSimpsonIntegral
                .integrate(&[1.0], &[1.0, 2.0])
                .is_err()
        );
    }

    #[test]
    fn sampled_formulae_reject_non_finite_values() {
        assert!(
            DiscreteTrapezoidIntegral
                .integrate(&[0.0, Real::NAN], &[1.0, 2.0])
                .is_err()
        );
        assert!(
            DiscreteSimpsonIntegral
                .integrate(&[0.0, 1.0, 2.0], &[1.0, Real::INFINITY, 3.0])
                .is_err()
        );
    }

    #[test]
    fn sampled_simpson_rejects_duplicate_adjacent_x() {
        assert!(
            DiscreteSimpsonIntegral
                .integrate(&[0.0, 1.0, 1.0], &[1.0, 2.0, 3.0])
                .is_err()
        );
    }

    const TOL: Real = 1e-6;

    // Port of testDiscreteIntegrator / testSeveral (Abcd case omitted): both
    // grid integrators reproduce the standard integrals within tolerance.
    fn check_several(integrate: impl Fn(&dyn Fn(Real) -> Real, Real, Real) -> Real) {
        assert!((integrate(&|_| 0.0, 0.0, 1.0) - 0.0).abs() < TOL);
        assert!((integrate(&|_| 1.0, 0.0, 1.0) - 1.0).abs() < TOL);
        assert!((integrate(&|x| x, 0.0, 1.0) - 0.5).abs() < TOL);
        assert!((integrate(&|x| x * x, 0.0, 1.0) - 1.0 / 3.0).abs() < TOL);
        assert!((integrate(&|x| x.sin(), 0.0, std::f64::consts::PI) - 2.0).abs() < TOL);
        assert!((integrate(&|x| x.cos(), 0.0, std::f64::consts::PI) - 0.0).abs() < TOL);
        let g = NormalDistribution::standard();
        assert!((integrate(&|x| g.value(x), -10.0, 10.0) - 1.0).abs() < TOL);
    }

    #[test]
    fn grid_simpson_reproduces_known_integrals() {
        let integ = DiscreteSimpsonIntegrator::new(300).unwrap();
        check_several(|f, a, b| integ.integrate(f, a, b).unwrap());
    }

    #[test]
    fn grid_trapezoid_reproduces_known_integrals() {
        let integ = DiscreteTrapezoidIntegrator::new(3000).unwrap();
        check_several(|f, a, b| integ.integrate(f, a, b).unwrap());
    }

    #[test]
    fn grid_integrators_reject_too_few_evaluations() {
        assert!(DiscreteTrapezoidIntegrator::new(1).is_err());
        // Simpson needs at least two intervals: a single interval (2 nodes)
        // double-counts f(a) in the tail and returns 5/3 for f == 1 over [0, 1].
        assert!(DiscreteSimpsonIntegrator::new(2).is_err());
        // Three nodes is the smallest valid Simpson grid and is exact on it.
        let simpson = DiscreteSimpsonIntegrator::new(3).unwrap();
        assert!((simpson.integrate(|_| 1.0, 0.0, 1.0).unwrap() - 1.0).abs() < TOL);
    }
}
