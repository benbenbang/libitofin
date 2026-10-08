use super::*;
use crate::{Flow, GlobalOptions, IterationState, Method};
use std::io;

#[path = "firefly_numeric_tests.rs"]
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

fn options() -> FireflyOptions {
    FireflyOptions {
        global: GlobalOptions {
            initial_population: Some(vec![vec![-2.0], vec![-1.0], vec![1.0], vec![2.0]]),
            xatol: Some(0.0),
            fatol: Some(0.0),
            ..GlobalOptions::default()
        },
        ..FireflyOptions::default()
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
    fn value(&mut self, x: &[f64]) -> Result<f64, Self::Error> {
        self.points.push(x.to_vec());
        if self.error_at == Some(self.points.len()) {
            return Err(io::Error::other("objective marker"));
        }
        if let Some((at, f)) = self.nonfinite_at
            && at == self.points.len()
        {
            return Ok(f);
        }
        Ok(if self.constant {
            -3.0
        } else {
            (x[0] - 0.25).powi(2) - 3.0
        })
    }
    fn gradient(&mut self, _: &[f64], _: &mut [f64]) -> Result<bool, Self::Error> {
        panic!("firefly must not request gradients")
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
            return Err(io::Error::other("callback marker"));
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
    p: &Problem,
    options: FireflyOptions,
    common: Common,
) -> Minimize {
    crate::minimize(recorder, p, &Method::Firefly(options), &common).unwrap()
}

#[test]
fn initial_population_evaluates_unchanged_in_supplied_order_even_duplicates() {
    let mut opts = options();
    opts.global.initial_population.as_mut().unwrap()[1] = vec![-2.0];
    let mut objective = Recorder::default();
    let result = solve(
        &mut objective,
        &problem(),
        opts.clone(),
        Common {
            maxfev: Some(4),
            ..Common::default()
        },
    );
    assert_eq!(objective.points, opts.global.initial_population.unwrap());
    assert_eq!((result.nit, result.nfev, result.njev), (0, 4, 0));
    assert_eq!(result.status, Termination::MaxEvaluations);
    assert!(objective.callbacks.is_empty());
}

#[test]
fn every_initial_and_partial_generation_budget_keeps_best_actual_call() {
    for budget in 1..20 {
        let mut objective = Recorder::default();
        let result = solve(
            &mut objective,
            &problem(),
            options(),
            Common {
                maxfev: Some(budget),
                ..Common::default()
            },
        );
        assert_eq!(result.nfev, budget);
        assert_eq!(objective.points.len(), budget);
        assert_eq!(result.nit, objective.callbacks.len());
        assert_eq!(result.njev, 0);
        assert_eq!(result.status, Termination::MaxEvaluations);
        let min = objective
            .points
            .iter()
            .map(|x| (x[0] - 0.25).powi(2) - 3.0)
            .fold(f64::INFINITY, f64::min);
        assert_eq!(result.fun, min);
        assert_eq!(result.fun, (result.x[0] - 0.25).powi(2) - 3.0);
    }
}

#[test]
fn hand_generation_uses_old_brightness_old_targets_and_current_moving_point() {
    let mut objective = Recorder::default();
    let opts = FireflyOptions {
        alpha: 0.0,
        beta0: 0.5,
        gamma: 0.0,
        ..options()
    };
    let result = solve(
        &mut objective,
        &problem(),
        opts,
        Common {
            maxiter: Some(1),
            ..Common::default()
        },
    );
    assert_eq!(
        objective.points,
        vec![
            vec![-2.0],
            vec![-1.0],
            vec![1.0],
            vec![2.0],
            vec![-1.5],
            vec![-0.25],
            vec![0.875],
            vec![0.0],
            vec![0.5],
            vec![0.75]
        ]
    );
    assert_eq!((result.nit, result.nfev, result.njev), (1, 10, 0));
    assert_eq!(result.status, Termination::MaxIterations);
    assert_eq!(result.x, vec![0.0]);
    assert_eq!(result.fun, -2.9375);
    assert_eq!(objective.callbacks, vec![(vec![0.0], -2.9375, 1, 10, 0)]);
}

#[test]
fn callback_reports_archive_best_and_exact_completed_generation_counts() {
    let mut objective = Recorder::default();
    let result = solve(
        &mut objective,
        &problem(),
        options(),
        Common {
            maxiter: Some(3),
            ..Common::default()
        },
    );
    assert_eq!(
        (result.nit, result.nfev, result.njev),
        (3, objective.points.len(), 0)
    );
    assert_eq!(result.status, Termination::MaxIterations);
    assert_eq!(objective.callbacks.len(), 3);
    for (i, (x, f, nit, nfev, njev)) in objective.callbacks.iter().enumerate() {
        assert_eq!((*nit, *njev), (i + 1, 0));
        assert_eq!(*f, (x[0] - 0.25).powi(2) - 3.0);
        let best = objective.points[..*nfev]
            .iter()
            .map(|x| (x[0] - 0.25).powi(2) - 3.0)
            .fold(f64::INFINITY, f64::min);
        assert_eq!(*f, best);
    }
}

#[test]
fn cancellation_wins_iteration_budget_and_errors_keep_original_values() {
    let mut objective = Recorder {
        stop: true,
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
    assert_eq!((result.nit, result.nfev), (1, 11));
    assert_eq!(result.status, Termination::Cancelled);
    assert!(!result.success);
    for at in [1, 3, 5, 10] {
        let mut objective = Recorder {
            error_at: Some(at),
            ..Recorder::default()
        };
        let error = crate::minimize(
            &mut objective,
            &problem(),
            &Method::Firefly(options()),
            &Common::default(),
        )
        .unwrap_err();
        match error {
            MinimizeError::Objective(error) => assert_eq!(error.to_string(), "objective marker"),
            other => panic!("{other:?}"),
        }
        assert_eq!(objective.points.len(), at);
    }
    let mut objective = Recorder {
        callback_error: true,
        ..Recorder::default()
    };
    let error = crate::minimize(
        &mut objective,
        &problem(),
        &Method::Firefly(options()),
        &Common::default(),
    )
    .unwrap_err();
    match error {
        MinimizeError::Objective(error) => assert_eq!(error.to_string(), "callback marker"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn nonfinite_evaluations_terminate_without_erasing_finite_archive() {
    for at in [1, 3, 5] {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut objective = Recorder {
                nonfinite_at: Some((at, value)),
                ..Recorder::default()
            };
            let result = solve(&mut objective, &problem(), options(), Common::default());
            assert_eq!(result.status, Termination::Nonfinite);
            assert_eq!(result.nfev, at);
            assert_eq!(result.nit, 0);
            if at == 1 {
                assert_eq!(result.x, vec![-2.0]);
                assert_eq!(result.fun.to_bits(), value.to_bits());
            } else {
                assert!(result.fun.is_finite());
            }
        }
    }
}

#[test]
fn ties_do_not_attract_and_archive_keeps_first_evaluated_point() {
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
    assert_eq!((result.nit, result.nfev), (1, 8));
    assert_eq!(result.x, vec![-2.0]);
    assert_eq!(result.fun, -3.0);
    let mut objective = Recorder {
        constant: true,
        ..Recorder::default()
    };
    let result = solve(
        &mut objective,
        &problem(),
        FireflyOptions {
            alpha: 0.0,
            ..options()
        },
        Common {
            maxiter: Some(3),
            ..Common::default()
        },
    );
    assert_eq!((result.nit, result.nfev), (3, 4));
    assert_eq!(result.status, Termination::MaxIterations);
}

#[test]
fn all_fixed_box_evaluates_x0_once_after_full_input_validation() {
    let p = Problem {
        x0: vec![7.0],
        bounds: Some(Bounds {
            lower: vec![7.0],
            upper: vec![7.0],
        }),
    };
    let mut objective = Recorder::default();
    let result = solve(
        &mut objective,
        &p,
        FireflyOptions::default(),
        Common::default(),
    );
    assert_eq!(objective.points, vec![vec![7.0]]);
    assert_eq!((result.nit, result.nfev, result.njev), (0, 1, 0));
    assert_eq!(result.status, Termination::Converged(Converged::XTol));
    assert!(objective.callbacks.is_empty());
}
