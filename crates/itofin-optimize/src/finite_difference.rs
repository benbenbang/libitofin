//! Finite-difference gradients for objectives without an analytic one.
//!
//! The step along coordinate `i` is relative, `h_i = c * max(1, |x_i|)`, with
//! `c = sqrt(eps)` for forward differences and `c = eps^(1/3)` for central
//! differences. The quotient divides by the step actually taken,
//! `(x_i + h_i) - x_i`, so the rounding of the perturbed coordinate does not
//! leak into the derivative.
//!
//! - Nocedal, J. and Wright, S. J. (2006), Numerical Optimization, 2nd edition,
//!   Springer, Section 8.1: the forward and central formulas and their step
//!   rules.
//!
//! OPT-5 (#1083) is the first caller and removes the `dead_code` allowance.
#![cfg_attr(not(test), allow(dead_code))]

use crate::counters::{Counters, Halt};
use crate::objective::Objective;
use crate::outcome::Termination;

/// How a gradient is approximated when the objective supplies none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FiniteDifference {
    /// `(f(x + h e_i) - f(x)) / h`: one extra evaluation per coordinate.
    Forward,
    /// `(f(x + h e_i) - f(x - h e_i)) / 2h`: two extra evaluations per
    /// coordinate, second-order accurate.
    Central,
}

impl FiniteDifference {
    fn relative_step(self) -> f64 {
        match self {
            FiniteDifference::Forward => f64::EPSILON.sqrt(),
            FiniteDifference::Central => f64::EPSILON.cbrt(),
        }
    }
}

/// Writes the gradient at `x`, where the objective takes the value `fx`, into
/// `out`.
///
/// An analytic gradient is used when the objective supplies one. Otherwise the
/// gradient is approximated by `scheme`, charging every extra evaluation to
/// `nfev` through `counters`. Either way one gradient evaluation is charged to
/// `njev`, but only once the gradient is complete.
///
/// Any nonfinite value met on the way, `+inf` included, ends the run with
/// [`Termination::Nonfinite`], because it leaves a nonfinite component in the
/// gradient; so does a nonfinite analytic component.
///
/// # Errors
///
/// A [`Halt`] when the evaluation budget runs out, the objective fails or a
/// value is nonfinite. `out` then holds a partial gradient and must not be
/// used. A finite-difference gradient cut short is not charged; an analytic
/// gradient with a nonfinite component already was.
pub(crate) fn gradient<O: Objective>(
    counters: &mut Counters,
    objective: &mut O,
    x: &[f64],
    fx: f64,
    scheme: FiniteDifference,
    out: &mut [f64],
) -> Result<(), Halt<O::Error>> {
    if !counters.gradient(objective, x, out)? {
        let mut point = x.to_vec();
        for (i, &xi) in x.iter().enumerate() {
            let h = scheme.relative_step() * xi.abs().max(1.0);
            let plus = xi + h;
            point[i] = plus;
            let f_plus = finite(counters.value(objective, &point)?)?;
            out[i] = match scheme {
                FiniteDifference::Forward => (f_plus - fx) / (plus - xi),
                FiniteDifference::Central => {
                    let minus = xi - h;
                    point[i] = minus;
                    let f_minus = finite(counters.value(objective, &point)?)?;
                    (f_plus - f_minus) / (plus - minus)
                }
            };
            point[i] = xi;
        }
        counters.charge_gradient();
    }
    if out.iter().all(|component| component.is_finite()) {
        Ok(())
    } else {
        Err(Halt::Terminated(Termination::Nonfinite))
    }
}

fn finite<E: std::error::Error + 'static>(value: f64) -> Result<f64, Halt<E>> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Halt::Terminated(Termination::Nonfinite))
    }
}

#[cfg(test)]
mod tests {
    use super::{FiniteDifference, gradient};
    use crate::{Common, Counters, Halt, Objective, Termination};
    use std::convert::Infallible;

    fn quadratic(x: &[f64]) -> Result<f64, Infallible> {
        Ok(1.5 * x[0] * x[0] + x[0] * x[1] + x[1] * x[1] - 2.0 * x[1])
    }

    fn quadratic_gradient(x: &[f64]) -> [f64; 2] {
        [3.0 * x[0] + x[1], x[0] + 2.0 * x[1] - 2.0]
    }

    fn smooth(x: &[f64]) -> Result<f64, Infallible> {
        Ok(x[0].exp() + x[1].sin() + x[0] * x[1])
    }

    fn smooth_gradient(x: &[f64]) -> [f64; 2] {
        [x[0].exp() + x[1], x[1].cos() + x[0]]
    }

    fn approximate<O: Objective>(
        objective: &mut O,
        x: &[f64],
        scheme: FiniteDifference,
        common: &Common,
    ) -> (Result<Vec<f64>, Halt<O::Error>>, Counters) {
        let mut counters = Counters::new(common);
        let fx = objective.value(x).unwrap_or(0.0);
        let mut out = vec![0.0; x.len()];
        let result = gradient(&mut counters, objective, x, fx, scheme, &mut out).map(|()| out);
        (result, counters)
    }

    fn assert_close(found: &[f64], expected: &[f64], tolerance: f64) {
        for (found, expected) in found.iter().zip(expected) {
            assert!(
                (found - expected).abs() <= tolerance,
                "{found} vs {expected} beyond {tolerance}"
            );
        }
    }

    #[test]
    fn forward_gradient_of_a_quadratic_is_exact_to_1e_6() {
        for x in [[1.5, -2.0], [0.0, 0.0], [-4.0, 2.5]] {
            let (g, _) = approximate(
                &mut quadratic,
                &x,
                FiniteDifference::Forward,
                &Common::default(),
            );
            assert_close(&g.unwrap(), &quadratic_gradient(&x), 1e-6);
        }
    }

    #[test]
    fn central_gradient_of_a_quadratic_is_exact_to_1e_9() {
        for x in [[1.5, -2.0], [0.0, 0.0], [-2.0, 3.0]] {
            let (g, _) = approximate(
                &mut quadratic,
                &x,
                FiniteDifference::Central,
                &Common::default(),
            );
            assert_close(&g.unwrap(), &quadratic_gradient(&x), 1e-9);
        }
    }

    #[test]
    fn central_gradient_with_a_nonzero_third_derivative_is_exact_to_1e_9() {
        for x in [[0.5, 1.0], [-1.0, 2.5], [1.2, -0.3]] {
            let (g, _) = approximate(
                &mut smooth,
                &x,
                FiniteDifference::Central,
                &Common::default(),
            );
            assert_close(&g.unwrap(), &smooth_gradient(&x), 1e-9);
        }
    }

    #[test]
    fn forward_charges_n_values_and_one_gradient() {
        let (g, counters) = approximate(
            &mut quadratic,
            &[1.0, 2.0],
            FiniteDifference::Forward,
            &Common::default(),
        );
        assert!(g.is_ok());
        assert_eq!((counters.nfev(), counters.njev()), (2, 1));
    }

    #[test]
    fn central_charges_2n_values_and_one_gradient() {
        let (g, counters) = approximate(
            &mut quadratic,
            &[1.0, 2.0],
            FiniteDifference::Central,
            &Common::default(),
        );
        assert!(g.is_ok());
        assert_eq!((counters.nfev(), counters.njev()), (4, 1));
    }

    struct Analytic;

    impl Objective for Analytic {
        type Error = Infallible;

        fn value(&mut self, x: &[f64]) -> Result<f64, Self::Error> {
            quadratic(x)
        }

        fn gradient(&mut self, x: &[f64], out: &mut [f64]) -> Result<bool, Self::Error> {
            out.copy_from_slice(&quadratic_gradient(x));
            Ok(true)
        }
    }

    #[test]
    fn an_analytic_gradient_costs_no_value_and_one_gradient() {
        let (g, counters) = approximate(
            &mut Analytic,
            &[1.0, 2.0],
            FiniteDifference::Central,
            &Common::default(),
        );
        assert_eq!(g.unwrap(), quadratic_gradient(&[1.0, 2.0]));
        assert_eq!((counters.nfev(), counters.njev()), (0, 1));
    }

    #[test]
    fn a_budget_refused_mid_gradient_charges_no_gradient() {
        let common = Common {
            maxfev: Some(3),
            ..Common::default()
        };
        let (g, counters) = approximate(
            &mut quadratic,
            &[1.0, 2.0],
            FiniteDifference::Central,
            &common,
        );
        assert!(matches!(
            g,
            Err(Halt::Terminated(Termination::MaxEvaluations))
        ));
        assert_eq!((counters.nfev(), counters.njev()), (3, 0));
    }

    #[test]
    fn a_nonfinite_value_ends_the_run_nonfinite() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut objective =
                |x: &[f64]| -> Result<f64, Infallible> { Ok(if x[1] > 2.0 { bad } else { x[0] }) };
            for scheme in [FiniteDifference::Forward, FiniteDifference::Central] {
                let (g, counters) =
                    approximate(&mut objective, &[1.0, 2.0], scheme, &Common::default());
                assert!(matches!(g, Err(Halt::Terminated(Termination::Nonfinite))));
                assert_eq!(counters.njev(), 0);
            }
        }
    }
}
