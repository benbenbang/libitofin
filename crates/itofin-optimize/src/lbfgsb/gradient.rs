//! Bound-aware finite differences for L-BFGS-B.

use crate::counters::{Counters, Halt};
use crate::finite_difference::FiniteDifference;
use crate::objective::Objective;
use crate::outcome::Termination;
use crate::problem::Bounds;

fn finite<E: std::error::Error + 'static>(value: f64) -> Result<f64, Halt<E>> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Halt::Terminated(Termination::Nonfinite))
    }
}

fn probe<O: Objective>(
    counters: &mut Counters,
    objective: &mut O,
    point: &mut [f64],
    index: usize,
    value: f64,
) -> Result<f64, Halt<O::Error>> {
    let original = point[index];
    point[index] = value;
    let result = counters.value(objective, point).and_then(finite);
    point[index] = original;
    result
}

#[derive(Clone, Copy)]
pub(super) struct Difference {
    pub scheme: FiniteDifference,
    pub eps: Option<f64>,
}

pub(super) fn bounded_gradient<O: Objective>(
    counters: &mut Counters,
    objective: &mut O,
    x: &[f64],
    fx: f64,
    bounds: &Bounds,
    difference: Difference,
    out: &mut [f64],
) -> Result<(), Halt<O::Error>> {
    if !counters.gradient(objective, x, out)? {
        let mut point = x.to_vec();
        for (i, &xi) in x.iter().enumerate() {
            if bounds.lower[i] == bounds.upper[i] {
                out[i] = 0.0;
                continue;
            }
            let relative = match difference.scheme {
                FiniteDifference::Forward => f64::EPSILON.sqrt(),
                FiniteDifference::Central => f64::EPSILON.cbrt(),
            };
            let h = difference.eps.unwrap_or(relative * xi.abs().max(1.0));
            let mut plus = (xi + h).min(bounds.upper[i]);
            let mut minus = (xi - h).max(bounds.lower[i]);
            if plus == xi && bounds.upper[i] > xi {
                plus = xi.next_up().min(bounds.upper[i]);
            }
            if minus == xi && bounds.lower[i] < xi {
                minus = xi.next_down().max(bounds.lower[i]);
            }
            let right = plus.is_finite() && plus > xi;
            let left = minus.is_finite() && minus < xi;
            out[i] = match (right, left, difference.scheme) {
                (true, true, FiniteDifference::Central) => {
                    let fp = probe(counters, objective, &mut point, i, plus)?;
                    let fm = probe(counters, objective, &mut point, i, minus)?;
                    (fp - fm) / (plus - minus)
                }
                (true, _, _) => {
                    let fp = probe(counters, objective, &mut point, i, plus)?;
                    (fp - fx) / (plus - xi)
                }
                (_, true, _) => {
                    let fm = probe(counters, objective, &mut point, i, minus)?;
                    (fx - fm) / (xi - minus)
                }
                (false, false, _) => 0.0,
            };
        }
        counters.charge_gradient();
    }
    if out.iter().all(|gi| gi.is_finite()) {
        Ok(())
    } else {
        Err(Halt::Terminated(Termination::Nonfinite))
    }
}

#[cfg(test)]
mod tests {
    use super::{Difference, bounded_gradient};
    use crate::{Bounds, Common, Counters, FiniteDifference};
    use std::convert::Infallible;

    #[test]
    fn differences_stay_within_the_box_and_skip_a_fixed_coordinate() {
        let bounds = Bounds {
            lower: vec![0.0, -1.0, 7.0],
            upper: vec![1.0, 1.0, 7.0],
        };
        let x = [1.0, -1.0, 7.0];
        let mut objective = |point: &[f64]| -> Result<f64, Infallible> {
            for (i, &xi) in point.iter().enumerate() {
                assert!(xi >= bounds.lower[i] && xi <= bounds.upper[i]);
            }
            Ok(point[0] * point[0] + point[1] * point[1] + point[2])
        };
        let mut counters = Counters::new(&Common::default());
        let fx = counters.value(&mut objective, &x).expect("initial value");
        let mut g = [0.0; 3];
        bounded_gradient(
            &mut counters,
            &mut objective,
            &x,
            fx,
            &bounds,
            Difference {
                scheme: FiniteDifference::Central,
                eps: Some(1e-6),
            },
            &mut g,
        )
        .expect("bounded gradient");
        assert!((g[0] - 2.0).abs() < 1e-5);
        assert!((g[1] + 2.0).abs() < 1e-5);
        assert_eq!(g[2], 0.0);
        assert_eq!((counters.nfev(), counters.njev()), (3, 1));
    }

    #[test]
    fn a_sub_ulp_step_still_probes_a_feasible_neighbor() {
        let bounds = Bounds {
            lower: vec![0.0],
            upper: vec![1.0],
        };
        let mut objective = |x: &[f64]| -> Result<f64, Infallible> { Ok(-x[0]) };
        let mut counters = Counters::new(&Common::default());
        let mut g = [0.0];
        bounded_gradient(
            &mut counters,
            &mut objective,
            &[1.0],
            -1.0,
            &bounds,
            Difference {
                scheme: FiniteDifference::Forward,
                eps: Some(f64::MIN_POSITIVE),
            },
            &mut g,
        )
        .expect("one-sided neighbor");
        assert_eq!(g[0], -1.0);
        assert_eq!((counters.nfev(), counters.njev()), (1, 1));
    }
}
