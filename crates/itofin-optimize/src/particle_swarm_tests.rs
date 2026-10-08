use super::*;
use crate::{Flow, GlobalOptions, IterationState, Method};
use std::io;

fn problem() -> Problem {
    Problem {
        x0: vec![0.0],
        bounds: Some(Bounds {
            lower: vec![-2.0],
            upper: vec![2.0],
        }),
    }
}

fn options() -> ParticleSwarmOptions {
    ParticleSwarmOptions {
        global: GlobalOptions {
            initial_population: Some(vec![vec![-2.0], vec![-1.0], vec![1.0], vec![2.0]]),
            xatol: Some(0.0),
            fatol: Some(0.0),
            ..GlobalOptions::default()
        },
        ..ParticleSwarmOptions::default()
    }
}

#[derive(Default)]
struct Recorder {
    points: Vec<Vec<f64>>,
    callbacks: Vec<(Vec<f64>, f64, usize, usize, usize)>,
    value_error_at: Option<usize>,
    nonfinite_at: Option<(usize, f64)>,
    stop: bool,
    callback_error: bool,
    constant: bool,
}

impl Objective for Recorder {
    type Error = io::Error;
    fn value(&mut self, x: &[f64]) -> Result<f64, Self::Error> {
        self.points.push(x.to_vec());
        if self.value_error_at == Some(self.points.len()) {
            return Err(io::Error::other("objective marker"));
        }
        if let Some((at, value)) = self.nonfinite_at
            && at == self.points.len()
        {
            return Ok(value);
        }
        Ok(if self.constant {
            -3.0
        } else {
            (x[0] - 0.25).powi(2) - 3.0
        })
    }
    fn gradient(&mut self, _: &[f64], _: &mut [f64]) -> Result<bool, Self::Error> {
        panic!("particle swarm must never request gradients")
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
    opts: ParticleSwarmOptions,
    common: Common,
) -> Minimize {
    crate::minimize(recorder, p, &Method::ParticleSwarm(opts), &common).unwrap()
}

#[test]
fn initialization_is_physical_and_preserves_supplied_order() {
    let mut objective = Recorder::default();
    let result = solve(
        &mut objective,
        &problem(),
        options(),
        Common {
            maxfev: Some(4),
            ..Common::default()
        },
    );
    assert_eq!(
        objective.points,
        options().global.initial_population.unwrap()
    );
    assert_eq!((result.nit, result.nfev, result.njev), (0, 4, 0));
    assert_eq!(result.x, vec![1.0]);
    assert_eq!(result.status, Termination::MaxEvaluations);
    assert!(objective.callbacks.is_empty());
}

#[test]
fn initialization_budget_returns_best_evaluated_point_without_callback() {
    for budget in 1..4 {
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
        assert_eq!(result.nit, 0);
        assert_eq!(result.status, Termination::MaxEvaluations);
        let best = objective
            .points
            .iter()
            .min_by(|a, b| (a[0] - 0.25).abs().total_cmp(&(b[0] - 0.25).abs()))
            .unwrap();
        assert_eq!(&result.x, best);
        assert!(objective.callbacks.is_empty());
    }
}

#[test]
fn partial_generation_updates_best_without_calling_completion_callback() {
    let mut objective = Recorder::default();
    let result = solve(
        &mut objective,
        &problem(),
        options(),
        Common {
            maxfev: Some(11),
            ..Common::default()
        },
    );
    assert_eq!((result.nit, result.nfev, result.njev), (1, 11, 0));
    assert_eq!(objective.callbacks.len(), 1);
    assert_eq!(result.status, Termination::MaxEvaluations);
    let min = objective
        .points
        .iter()
        .map(|x| (x[0] - 0.25).powi(2) - 3.0)
        .fold(f64::INFINITY, f64::min);
    assert_eq!(result.fun, min);
}

#[test]
fn callbacks_report_best_and_exact_completed_generation_counts() {
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
    assert_eq!((result.nit, result.nfev, result.njev), (3, 16, 0));
    assert_eq!(result.status, Termination::MaxIterations);
    for (i, (x, fun, nit, nfev, njev)) in objective.callbacks.iter().enumerate() {
        assert_eq!((*nit, *nfev, *njev), (i + 1, 4 * (i + 2), 0));
        assert_eq!(*fun, (x[0] - 0.25).powi(2) - 3.0);
        let min = objective.points[..*nfev]
            .iter()
            .map(|p| (p[0] - 0.25).powi(2) - 3.0)
            .fold(f64::INFINITY, f64::min);
        assert_eq!(*fun, min);
    }
}

#[test]
fn callback_cancellation_wins_over_iteration_budget() {
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
    assert_eq!((result.nit, result.nfev, result.njev), (1, 8, 0));
    assert_eq!(result.status, Termination::Cancelled);
    assert!(!result.success);
}

#[test]
fn objective_and_callback_failures_keep_original_error_values() {
    for at in [1, 3, 6] {
        let mut objective = Recorder {
            value_error_at: Some(at),
            ..Recorder::default()
        };
        let error = crate::minimize(
            &mut objective,
            &problem(),
            &Method::ParticleSwarm(options()),
            &Common::default(),
        )
        .unwrap_err();
        assert!(
            matches!(error, MinimizeError::Objective(ref e) if e.to_string()=="objective marker")
        );
        assert_eq!(objective.points.len(), at);
        assert!(objective.callbacks.is_empty());
    }
    let mut objective = Recorder {
        callback_error: true,
        ..Recorder::default()
    };
    let error = crate::minimize(
        &mut objective,
        &problem(),
        &Method::ParticleSwarm(options()),
        &Common::default(),
    )
    .unwrap_err();
    assert!(matches!(error, MinimizeError::Objective(ref e) if e.to_string()=="callback marker"));
    assert_eq!(objective.points.len(), 8);
    assert_eq!(objective.callbacks.len(), 1);
}

#[test]
fn nonfinite_status_retains_best_finite_point_and_counts_failed_evaluation() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for at in [1, 3, 6] {
            let mut objective = Recorder {
                nonfinite_at: Some((at, value)),
                ..Recorder::default()
            };
            let result = solve(&mut objective, &problem(), options(), Common::default());
            assert_eq!(result.status, Termination::Nonfinite);
            assert_eq!((result.nit, result.nfev, result.njev), (0, at, 0));
            assert!(!result.success);
            if at == 1 {
                assert_eq!(result.x, vec![-2.0]);
                assert!(result.fun.is_nan() || result.fun == value);
            } else {
                assert!(result.fun.is_finite());
                assert_eq!(
                    result.fun,
                    objective.points[..at - 1]
                        .iter()
                        .map(|p| (p[0] - 0.25).powi(2) - 3.0)
                        .fold(f64::INFINITY, f64::min)
                );
            }
        }
    }
}

#[test]
fn equal_values_retain_first_global_best() {
    let mut objective = Recorder {
        constant: true,
        ..Recorder::default()
    };
    let result = solve(
        &mut objective,
        &problem(),
        options(),
        Common {
            maxiter: Some(2),
            ..Common::default()
        },
    );
    assert_eq!(result.x, vec![-2.0]);
    assert_eq!(result.fun, -3.0);
    assert!(objective.callbacks.iter().all(|(x, ..)| x == &vec![-2.0]));
}

#[test]
fn same_seed_replays_random_initialization_and_all_objective_calls() {
    let mut opts = options();
    opts.global.initial_population = None;
    opts.global.population_size = Some(8);
    let common = Common {
        maxiter: Some(3),
        ..Common::default()
    };
    let mut first = Recorder::default();
    let mut second = Recorder::default();
    let a = solve(&mut first, &problem(), opts.clone(), common.clone());
    let b = solve(&mut second, &problem(), opts.clone(), common.clone());
    assert_eq!(first.points, second.points);
    assert_eq!(first.callbacks, second.callbacks);
    assert_eq!(
        (a.x, a.fun, a.status, a.nfev),
        (b.x, b.fun, b.status, b.nfev)
    );
    assert_eq!(first.points[0], problem().x0);
    opts.global.seed = 1;
    let mut third = Recorder::default();
    solve(&mut third, &problem(), opts, common);
    assert_ne!(first.points, third.points);
}

#[test]
fn all_fixed_box_evaluates_x0_once_after_validation() {
    let p = Problem {
        x0: vec![7.0],
        bounds: Some(Bounds {
            lower: vec![7.0],
            upper: vec![7.0],
        }),
    };
    let mut opts = options();
    opts.global.initial_population = Some(vec![vec![7.0]; 4]);
    let mut objective = Recorder::default();
    let result = solve(
        &mut objective,
        &p,
        opts.clone(),
        Common {
            maxfev: Some(1),
            ..Common::default()
        },
    );
    assert_eq!((result.nit, result.nfev, result.njev), (0, 1, 0));
    assert!(result.success);
    assert_eq!(objective.points, vec![p.x0.clone()]);
    opts.social = f64::NAN;
    assert!(opts.validate(&p).is_err());
    opts.social = 1.4;
    opts.global.initial_population.as_mut().unwrap()[3] = vec![8.0];
    assert!(opts.validate(&p).is_err());
}
