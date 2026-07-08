//! Adaptive Gauss-Lobatto integration.
//!
//! Port of `GaussLobattoIntegral` from
//! `ql/math/integrals/gausslobattointegral.{hpp,cpp}`, the Gander-Gautschi
//! adaptive Gauss-Lobatto/Kronrod scheme (Gander & Gautschi, "Adaptive
//! Quadrature - Revisited", BIT 40(1), 2000). It compares a 4-point Lobatto and
//! a 7-point Kronrod estimate on each interval and recursively subdivides until
//! they agree to the derived tolerance.

#![allow(clippy::excessive_precision)]

use crate::errors::QlResult;
use crate::fail;
use crate::math::integrals::{Integrator, require_accuracy};
use crate::types::{Real, Size};

// Lobatto/Kronrod node positions on the half-interval. ALPHA = sqrt(2/3),
// BETA = 1/sqrt(5) (QuantLib computes these at load time; the literals are their
// exact f64 values).
const ALPHA: Real = 0.816496580927726;
const BETA: Real = 0.4472135954999579;
const X1: Real = 0.94288241569547971906;
const X2: Real = 0.64185334234578130578;
const X3: Real = 0.23638319966214988028;

/// An interval `[a, b]` with the integrand's values at its endpoints.
#[derive(Clone, Copy)]
struct Panel {
    a: Real,
    b: Real,
    fa: Real,
    fb: Real,
}

/// Adaptive Gauss-Lobatto integrator.
pub struct GaussLobattoIntegral {
    max_evaluations: Size,
    abs_accuracy: Real,
    rel_accuracy: Option<Real>,
    use_convergence_estimate: bool,
}

impl GaussLobattoIntegral {
    /// A new integrator with the given evaluation budget and absolute accuracy
    /// (finite and above machine epsilon). No relative accuracy is set and the
    /// convergence-rate estimate is enabled, matching QuantLib's defaults.
    ///
    /// `max_evaluations` must be at least 15: deriving the working tolerance
    /// always spends 13 calls plus the two endpoints, so a smaller budget could
    /// not be honoured. QuantLib leaves this unchecked.
    pub fn new(max_evaluations: Size, abs_accuracy: Real) -> QlResult<Self> {
        require_accuracy(abs_accuracy)?;
        if max_evaluations < 15 {
            fail!("required max evaluations ({max_evaluations}) must be >= 15");
        }
        Ok(GaussLobattoIntegral {
            max_evaluations,
            abs_accuracy,
            rel_accuracy: None,
            use_convergence_estimate: true,
        })
    }

    /// Sets a relative accuracy target (finite and non-negative), in addition to
    /// the absolute one.
    pub fn with_relative_accuracy(mut self, rel_accuracy: Real) -> QlResult<Self> {
        if !rel_accuracy.is_finite() || rel_accuracy < 0.0 {
            fail!("relative accuracy must be finite and non-negative, got {rel_accuracy}");
        }
        self.rel_accuracy = Some(rel_accuracy);
        Ok(self)
    }

    /// Sets whether the initial tolerance uses the convergence-rate estimate.
    pub fn with_convergence_estimate(mut self, enabled: bool) -> Self {
        self.use_convergence_estimate = enabled;
        self
    }

    /// Derives the working absolute tolerance from a coarse high-order estimate
    /// of the integral and (optionally) a convergence-rate factor, per the
    /// Gander-Gautschi initialization.
    fn calculate_abs_tolerance<F>(
        &self,
        f: &mut F,
        a: Real,
        b: Real,
        evaluations: &mut Size,
    ) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        let m = (a + b) / 2.0;
        let h = (b - a) / 2.0;
        let y1 = f(a);
        let y3 = f(m - ALPHA * h);
        let y5 = f(m - BETA * h);
        let y7 = f(m);
        let y9 = f(m + BETA * h);
        let y11 = f(m + ALPHA * h);
        let y13 = f(b);
        let f1 = f(m - X1 * h);
        let f2 = f(m + X1 * h);
        let f3 = f(m - X2 * h);
        let f4 = f(m + X2 * h);
        let f5 = f(m - X3 * h);
        let f6 = f(m + X3 * h);

        let acc = h
            * (0.0158271919734801831 * (y1 + y13)
                + 0.0942738402188500455 * (f1 + f2)
                + 0.1550719873365853963 * (y3 + y11)
                + 0.1888215739601824544 * (f3 + f4)
                + 0.1997734052268585268 * (y5 + y9)
                + 0.2249264653333395270 * (f5 + f6)
                + 0.2426110719014077338 * y7);
        *evaluations += 13;

        if acc == 0.0
            && (f1 != 0.0 || f2 != 0.0 || f3 != 0.0 || f4 != 0.0 || f5 != 0.0 || f6 != 0.0)
        {
            fail!("cannot calculate absolute accuracy from relative accuracy");
        }

        let mut r = 1.0;
        if self.use_convergence_estimate {
            let integral2 = (h / 6.0) * (y1 + y13 + 5.0 * (y5 + y9));
            let integral1 = (h / 1470.0)
                * (77.0 * (y1 + y13) + 432.0 * (y3 + y11) + 625.0 * (y5 + y9) + 672.0 * y7);
            if (integral2 - acc).abs() != 0.0 {
                r = (integral1 - acc).abs() / (integral2 - acc).abs();
            }
            if r == 0.0 || r > 1.0 {
                r = 1.0;
            }
        }

        Ok(match self.rel_accuracy {
            Some(rel) => {
                let rel_tol = rel.max(Real::EPSILON);
                self.abs_accuracy.min(acc * rel_tol) / (r * Real::EPSILON)
            }
            None => self.abs_accuracy / (r * Real::EPSILON),
        })
    }

    /// One adaptive step over `panel`: compare the 4-point Lobatto and 7-point
    /// Kronrod estimates and, if they disagree beyond `acc`, subdivide into six
    /// panels.
    // `dist == acc` is the algorithm's deliberate "have we hit the tolerance
    // floor" test (adding a term too small to change acc), not incidental drift.
    #[allow(clippy::float_cmp)]
    fn adaptive_step<F>(
        &self,
        f: &mut F,
        panel: Panel,
        acc: Real,
        evaluations: &mut Size,
    ) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        // Reject before spending this step's five evaluations if they would
        // overrun the budget. QuantLib only checks that the count is still below
        // the max here, so it can overshoot by up to five; we honour the
        // advertised budget exactly, which matters for costly or stateful `f`.
        if *evaluations + 5 > self.max_evaluations {
            fail!("maximum number of function evaluations reached");
        }
        let Panel { a, b, fa, fb } = panel;
        let h = (b - a) / 2.0;
        let m = (a + b) / 2.0;
        let mll = m - ALPHA * h;
        let ml = m - BETA * h;
        let mr = m + BETA * h;
        let mrr = m + ALPHA * h;
        let fmll = f(mll);
        let fml = f(ml);
        let fm = f(m);
        let fmr = f(mr);
        let fmrr = f(mrr);
        *evaluations += 5;

        let integral2 = (h / 6.0) * (fa + fb + 5.0 * (fml + fmr));
        let integral1 = (h / 1470.0)
            * (77.0 * (fa + fb) + 432.0 * (fmll + fmrr) + 625.0 * (fml + fmr) + 672.0 * fm);

        // Adding the discrepancy to acc and comparing avoids x86 80-bit issues.
        let dist = acc + (integral1 - integral2);
        if dist == acc || mll <= a || b <= mrr {
            if !(m > a && b > m) {
                fail!("interval contains no more machine numbers");
            }
            Ok(integral1)
        } else {
            let panels = [
                Panel {
                    a,
                    b: mll,
                    fa,
                    fb: fmll,
                },
                Panel {
                    a: mll,
                    b: ml,
                    fa: fmll,
                    fb: fml,
                },
                Panel {
                    a: ml,
                    b: m,
                    fa: fml,
                    fb: fm,
                },
                Panel {
                    a: m,
                    b: mr,
                    fa: fm,
                    fb: fmr,
                },
                Panel {
                    a: mr,
                    b: mrr,
                    fa: fmr,
                    fb: fmrr,
                },
                Panel {
                    a: mrr,
                    b,
                    fa: fmrr,
                    fb,
                },
            ];
            let mut total = 0.0;
            for p in panels {
                total += self.adaptive_step(f, p, acc, evaluations)?;
            }
            Ok(total)
        }
    }
}

impl Integrator for GaussLobattoIntegral {
    fn integrate_impl<F>(&self, f: &mut F, a: Real, b: Real) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        let mut evaluations: Size = 0;
        let acc = self.calculate_abs_tolerance(f, a, b, &mut evaluations)?;
        evaluations += 2;
        let (fa, fb) = (f(a), f(b));
        self.adaptive_step(f, Panel { a, b, fa, fb }, acc, &mut evaluations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::distributions::normal::NormalDistribution;

    const TOL: Real = 1e-6;

    #[test]
    fn matches_known_integrals() {
        // QuantLib's testSeveral (Abcd case omitted, not yet ported). The
        // degenerate domain is intentionally not tested: Lobatto throws there,
        // which QuantLib documents as acceptable.
        let gl = GaussLobattoIntegral::new(1000, TOL).unwrap();
        assert!((gl.integrate(|_| 1.0, 0.0, 1.0).unwrap() - 1.0).abs() < TOL);
        assert!((gl.integrate(|x| x, 0.0, 1.0).unwrap() - 0.5).abs() < TOL);
        assert!((gl.integrate(|x| x * x, 0.0, 1.0).unwrap() - 1.0 / 3.0).abs() < TOL);
        assert!(
            (gl.integrate(|x| x.sin(), 0.0, std::f64::consts::PI)
                .unwrap()
                - 2.0)
                .abs()
                < TOL
        );
        assert!(
            (gl.integrate(|x| x.cos(), 0.0, std::f64::consts::PI)
                .unwrap()
                - 0.0)
                .abs()
                < TOL
        );
        let g = NormalDistribution::standard();
        assert!((gl.integrate(|x| g.value(x), -10.0, 10.0).unwrap() - 1.0).abs() < TOL);
    }

    #[test]
    fn reversed_limits_negate() {
        let gl = GaussLobattoIntegral::new(1000, TOL).unwrap();
        assert!((gl.integrate(|x| x, 1.0, 0.0).unwrap() - (-0.5)).abs() < TOL);
    }

    #[test]
    fn relative_accuracy_and_convergence_options() {
        // The optional knobs still integrate the smooth cases correctly.
        let gl = GaussLobattoIntegral::new(1000, TOL)
            .unwrap()
            .with_relative_accuracy(TOL)
            .unwrap()
            .with_convergence_estimate(false);
        assert!((gl.integrate(|x| x * x, 0.0, 1.0).unwrap() - 1.0 / 3.0).abs() < TOL);
    }

    #[test]
    fn evaluation_budget_is_never_exceeded() {
        // Budgets below the 15-call tolerance initialization are rejected at
        // construction, so the integrator can never overspend during setup.
        for max in [0usize, 1, 14] {
            assert!(GaussLobattoIntegral::new(max, TOL).is_err(), "budget {max}");
        }
        // From 15 up, a step's five extra calls would overrun a tight budget, so
        // the pre-check stops before spending them; a counting integrand records
        // at most `max` calls (the old check let it reach 20).
        for max in 15..=19 {
            let gl = GaussLobattoIntegral::new(max, TOL).unwrap();
            let mut count = 0usize;
            let _ = gl.integrate(
                |x| {
                    count += 1;
                    x.sin()
                },
                0.0,
                std::f64::consts::PI,
            );
            assert!(count <= max, "used {count} evaluations, budget {max}");
        }
    }

    #[test]
    fn invalid_configuration_rejected() {
        for acc in [0.0, -1.0, Real::EPSILON, Real::NAN, Real::INFINITY] {
            assert!(GaussLobattoIntegral::new(1000, acc).is_err(), "abs={acc}");
        }
        let gl = GaussLobattoIntegral::new(1000, TOL).unwrap();
        assert!(gl.with_relative_accuracy(-1.0).is_err());
    }
}
