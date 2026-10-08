use super::*;
use crate::{Flow, IterationState, Method};
use std::io;

#[path = "simulated_annealing_numeric_tests.rs"]
mod numeric;

fn problem() -> Problem {
    Problem {
        x0: vec![0.0],
        bounds: Some(Bounds {
            lower: vec![-2.0],
            upper: vec![2.0],
        }),
    }
}

fn options() -> HybridSimulatedAnnealingOptions {
    HybridSimulatedAnnealingOptions {
        xatol: Some(0.0),
        fatol: Some(0.0),
        local_search_interval: 1,
        local_search_steps: 2,
        ..HybridSimulatedAnnealingOptions::default()
    }
}

#[derive(Default)]
struct Recorder {
    points: Vec<Vec<f64>>,
    callbacks: Vec<(Vec<f64>, f64, usize, usize, usize)>,
    error_at: Option<usize>,
    nonfinite_at: Option<(usize, f64)>,
    stop: bool,
    callback_error: bool,
    constant: bool,
}

impl Objective for Recorder {
    type Error = io::Error;

    fn value(&mut self, point: &[f64]) -> Result<f64, Self::Error> {
        self.points.push(point.to_vec());
        if self.error_at == Some(self.points.len()) {
            return Err(io::Error::other("annealing objective marker"));
        }
        if let Some((at, value)) = self.nonfinite_at
            && at == self.points.len()
        {
            return Ok(value);
        }
        Ok(if self.constant {
            -3.0
        } else {
            (point[0] - 0.25).powi(2) - 3.0
        })
    }

    fn gradient(&mut self, _: &[f64], _: &mut [f64]) -> Result<bool, Self::Error> {
        panic!("hybrid annealing must not request gradients")
    }

    fn callback(&mut self, state: &IterationState<'_>) -> Result<Flow, Self::Error> {
        self.callbacks.push((
            state.x.to_vec(),
            state.fun,
            state.nit,
            state.nfev,
            state.njev,
        ));
        if self.callback_error {
            return Err(io::Error::other("annealing callback marker"));
        }
        Ok(if self.stop {
            Flow::Stop
        } else {
            Flow::Continue
        })
    }
}

fn solve(
    recorder: &mut Recorder,
    problem: &Problem,
    options: HybridSimulatedAnnealingOptions,
    common: Common,
) -> Minimize {
    crate::minimize(
        recorder,
        problem,
        &Method::HybridSimulatedAnnealing(options),
        &common,
    )
    .unwrap()
}

#[test]
fn local_polls_are_positive_then_negative_and_only_unsuccessful_sweeps_shrink() {
    let mut objective = Recorder {
        constant: true,
        ..Recorder::default()
    };
    let result = solve(
        &mut objective,
        &problem(),
        options(),
        Common {
            maxiter: Some(1),
            ..Common::default()
        },
    );
    assert_eq!(
        &objective.points[2..],
        &[vec![1.0], vec![-1.0], vec![0.5], vec![-0.5]]
    );
    assert_eq!((result.nit, result.nfev, result.njev), (1, 6, 0));
    assert_eq!(result.x, vec![0.0]);
    assert_eq!(objective.callbacks, vec![(vec![0.0], -3.0, 1, 6, 0)]);
}

#[test]
fn budgets_cut_partial_local_sweeps_without_charging_an_iteration() {
    for budget in 1..=6 {
        let mut objective = Recorder {
            constant: true,
            ..Recorder::default()
        };
        let result = solve(
            &mut objective,
            &problem(),
            options(),
            Common {
                maxfev: Some(budget),
                ..Common::default()
            },
        );
        assert_eq!(result.status, Termination::MaxEvaluations);
        assert_eq!(result.nfev, budget);
        assert_eq!(objective.points.len(), budget);
        assert_eq!(result.nit, usize::from(budget == 6));
        assert_eq!(objective.callbacks.len(), result.nit);
        assert_eq!(result.x, vec![0.0]);
    }
}

#[test]
fn default_first_cycle_is_one_proposal_and_callback_with_exact_best() {
    let mut objective = Recorder::default();
    let result = solve(
        &mut objective,
        &problem(),
        HybridSimulatedAnnealingOptions::default(),
        Common {
            maxiter: Some(1),
            ..Common::default()
        },
    );
    assert_eq!((result.nit, result.nfev, result.njev), (1, 2, 0));
    assert_eq!(result.status, Termination::MaxIterations);
    assert_eq!(objective.callbacks[0], (result.x, result.fun, 1, 2, 0));
}

#[test]
fn callback_cancellation_wins_over_iteration_budget_and_convergence() {
    let mut objective = Recorder {
        constant: true,
        stop: true,
        ..Recorder::default()
    };
    let result = solve(
        &mut objective,
        &problem(),
        HybridSimulatedAnnealingOptions {
            xatol: Some(1.0),
            fatol: Some(0.0),
            ..options()
        },
        Common {
            maxiter: Some(1),
            ..Common::default()
        },
    );
    assert_eq!(result.status, Termination::Cancelled);
    assert_eq!((result.nit, result.nfev), (1, 4));
    assert!(!result.success);
}

#[test]
fn iteration_budget_precedes_pending_convergence_after_callback() {
    let mut objective = Recorder {
        constant: true,
        ..Recorder::default()
    };
    let result = solve(
        &mut objective,
        &problem(),
        HybridSimulatedAnnealingOptions {
            xatol: Some(1.0),
            fatol: Some(0.0),
            ..options()
        },
        Common {
            maxiter: Some(1),
            ..Common::default()
        },
    );
    assert_eq!(result.status, Termination::MaxIterations);
    assert_eq!((result.nit, result.nfev), (1, 4));
}

#[test]
fn typed_evaluation_and_callback_errors_are_not_penalty_values() {
    for at in [1, 2, 3, 4] {
        let mut objective = Recorder {
            error_at: Some(at),
            ..Recorder::default()
        };
        let result = crate::minimize(
            &mut objective,
            &problem(),
            &Method::HybridSimulatedAnnealing(options()),
            &Common::default(),
        );
        assert!(
            matches!(result, Err(MinimizeError::Objective(ref error)) if error.to_string() == "annealing objective marker")
        );
        assert_eq!(objective.points.len(), at);
    }
    let mut objective = Recorder {
        callback_error: true,
        ..Recorder::default()
    };
    let result = crate::minimize(
        &mut objective,
        &problem(),
        &Method::HybridSimulatedAnnealing(options()),
        &Common::default(),
    );
    assert!(
        matches!(result, Err(MinimizeError::Objective(ref error)) if error.to_string() == "annealing callback marker")
    );
    assert_eq!(objective.callbacks.len(), 1);
}

#[test]
fn nonfinite_evaluations_preserve_best_and_do_not_complete_partial_cycles() {
    for nonfinite in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for at in 1..=4 {
            let mut objective = Recorder {
                constant: true,
                nonfinite_at: Some((at, nonfinite)),
                ..Recorder::default()
            };
            let result = solve(&mut objective, &problem(), options(), Common::default());
            assert_eq!(result.status, Termination::Nonfinite);
            assert_eq!((result.nit, result.nfev), (0, at));
            assert!(objective.callbacks.is_empty());
            assert_eq!(result.x, vec![0.0]);
            if at == 1 {
                assert!(!result.fun.is_finite());
            } else {
                assert_eq!(result.fun, -3.0);
            }
        }
    }
}

#[test]
fn fixed_box_evaluates_once_without_callback_or_random_movement() {
    let fixed = Problem {
        x0: vec![1.0],
        bounds: Some(Bounds {
            lower: vec![1.0],
            upper: vec![1.0],
        }),
    };
    let mut objective = Recorder::default();
    let result = solve(&mut objective, &fixed, options(), Common::default());
    assert_eq!(result.status, Termination::Converged(Converged::XTol));
    assert_eq!((result.nit, result.nfev, result.njev), (0, 1, 0));
    assert_eq!(objective.points, vec![vec![1.0]]);
    assert!(objective.callbacks.is_empty());
}
