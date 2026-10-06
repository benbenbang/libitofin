use itofin_optimize::{
    Bounds, Common, DifferentialEvolutionOptions, Flow, GlobalOptions, IterationState, Method,
    Minimize, MinimizeError, Objective, Problem, Termination, minimize,
};
use std::convert::Infallible;

fn problem() -> Problem {
    Problem {
        x0: vec![0.0],
        bounds: Some(Bounds {
            lower: vec![-2.0],
            upper: vec![2.0],
        }),
    }
}

fn options() -> DifferentialEvolutionOptions {
    DifferentialEvolutionOptions {
        global: GlobalOptions {
            initial_population: Some(vec![vec![-2.0], vec![-1.0], vec![1.0], vec![2.0]]),
            xatol: Some(0.0),
            fatol: Some(0.0),
            ..GlobalOptions::default()
        },
        ..DifferentialEvolutionOptions::default()
    }
}

fn run(options: DifferentialEvolutionOptions, common: Common) -> Minimize {
    minimize(
        &mut |x: &[f64]| Ok::<_, Infallible>((x[0] - 0.37).powi(2) - 3.0),
        &problem(),
        &Method::DifferentialEvolution(options),
        &common,
    )
    .unwrap()
}

#[derive(Debug, PartialEq, Eq)]
struct Failure(u32);
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for Failure {}

struct Callbacks {
    values: usize,
    callbacks: usize,
    stop: bool,
    fail: bool,
}
impl Objective for Callbacks {
    type Error = Failure;
    fn value(&mut self, x: &[f64]) -> Result<f64, Self::Error> {
        self.values += 1;
        Ok(x[0] * x[0])
    }
    fn gradient(&mut self, _: &[f64], _: &mut [f64]) -> Result<bool, Self::Error> {
        panic!("DE must not request gradients")
    }
    fn callback(&mut self, state: &IterationState<'_>) -> Result<Flow, Self::Error> {
        self.callbacks += 1;
        assert_eq!(state.nit, self.callbacks);
        assert_eq!(state.nfev, self.values);
        assert_eq!(state.njev, 0);
        if self.fail {
            Err(Failure(19))
        } else if self.stop {
            Ok(Flow::Stop)
        } else {
            Ok(Flow::Continue)
        }
    }
}

#[test]
fn callbacks_only_run_after_a_full_generation_and_cancel_wins_over_budget() {
    let mut objective = Callbacks {
        values: 0,
        callbacks: 0,
        stop: true,
        fail: false,
    };
    let result = minimize(
        &mut objective,
        &problem(),
        &Method::DifferentialEvolution(options()),
        &Common {
            maxiter: Some(1),
            ..Common::default()
        },
    )
    .unwrap();
    assert_eq!(result.status, Termination::Cancelled);
    assert_eq!((result.nit, result.nfev, objective.callbacks), (1, 8, 1));
    let mut objective = Callbacks {
        values: 0,
        callbacks: 0,
        stop: true,
        fail: false,
    };
    let result = minimize(
        &mut objective,
        &problem(),
        &Method::DifferentialEvolution(options()),
        &Common {
            maxfev: Some(7),
            ..Common::default()
        },
    )
    .unwrap();
    assert_eq!((result.nit, result.nfev, objective.callbacks), (0, 7, 0));
}

#[test]
fn callback_and_objective_errors_are_returned_with_typed_payload() {
    let mut objective = Callbacks {
        values: 0,
        callbacks: 0,
        stop: false,
        fail: true,
    };
    let result = minimize(
        &mut objective,
        &problem(),
        &Method::DifferentialEvolution(options()),
        &Common::default(),
    );
    assert!(matches!(result, Err(MinimizeError::Objective(Failure(19)))));
    assert_eq!((objective.values, objective.callbacks), (8, 1));
    let result = minimize(
        &mut |_: &[f64]| Err::<f64, _>(Failure(23)),
        &problem(),
        &Method::DifferentialEvolution(options()),
        &Common::default(),
    );
    assert!(matches!(result, Err(MinimizeError::Objective(Failure(23)))));
}

#[test]
fn every_nonfinite_objective_stops_immediately_preserving_best_finite_point() {
    for nonfinite in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for bad_call in [1, 3, 6] {
            let mut calls = 0;
            let result = minimize(
                &mut |x: &[f64]| {
                    calls += 1;
                    Ok::<_, Infallible>(if calls == bad_call {
                        nonfinite
                    } else {
                        x[0] * x[0]
                    })
                },
                &problem(),
                &Method::DifferentialEvolution(options()),
                &Common::default(),
            )
            .unwrap();
            assert_eq!(result.status, Termination::Nonfinite);
            assert_eq!((result.nit, result.nfev, result.njev), (0, bad_call, 0));
            if bad_call == 1 {
                assert_eq!(result.x, vec![-2.0]);
                assert_eq!(result.fun.to_bits(), nonfinite.to_bits());
            } else {
                assert!(result.fun.is_finite());
                assert_eq!(result.fun, result.x[0] * result.x[0]);
                assert!(result.fun <= 1.0);
            }
        }
    }
}

#[test]
fn zero_crossover_still_mutates_a_free_coordinate() {
    let mut opts = options();
    opts.recombination = 0.0;
    let mut calls = Vec::new();
    let result = minimize(
        &mut |x: &[f64]| {
            calls.push(x[0]);
            Ok::<_, Infallible>(x[0] * x[0])
        },
        &problem(),
        &Method::DifferentialEvolution(opts),
        &Common {
            maxiter: Some(1),
            ..Common::default()
        },
    )
    .unwrap();
    assert_eq!(result.nfev, 8);
    assert!(
        calls[..4]
            .iter()
            .zip(&calls[4..])
            .any(|(parent, trial)| parent != trial)
    );
    assert!(calls.iter().all(|x| (-2.0..=2.0).contains(x)));
}

#[test]
fn both_spreads_are_required_and_common_tolerance_fills_unset_values() {
    let mut opts = options();
    opts.global.xatol = Some(1.0);
    let result = run(
        opts.clone(),
        Common {
            maxfev: Some(4),
            ..Common::default()
        },
    );
    assert_eq!(result.status, Termination::MaxEvaluations);
    opts.global.fatol = Some(100.0);
    assert!(run(opts.clone(), Common::default()).success);
    opts.global.xatol = Some(0.0);
    let result = run(
        opts.clone(),
        Common {
            maxfev: Some(4),
            ..Common::default()
        },
    );
    assert_eq!(result.status, Termination::MaxEvaluations);
    opts.global.xatol = None;
    opts.global.fatol = None;
    let result = run(
        opts,
        Common {
            tol: Some(100.0),
            ..Common::default()
        },
    );
    assert!(result.success);
    assert_eq!((result.nit, result.nfev), (0, 4));
}

#[test]
fn flat_objective_alone_does_not_converge_a_broad_population() {
    let result = minimize(
        &mut |_: &[f64]| Ok::<_, Infallible>(-7.0),
        &problem(),
        &Method::DifferentialEvolution(options()),
        &Common {
            maxfev: Some(4),
            ..Common::default()
        },
    )
    .unwrap();
    assert_eq!(result.status, Termination::MaxEvaluations);
    assert_eq!((result.nit, result.nfev), (0, 4));
}

#[test]
fn finite_objective_spread_overflow_never_counts_as_convergence() {
    let mut opts = options();
    opts.global.xatol = Some(1.0);
    opts.global.fatol = Some(f64::MAX);
    let result = minimize(
        &mut |x: &[f64]| Ok::<_, Infallible>(if x[0] < 0.0 { -f64::MAX } else { f64::MAX }),
        &problem(),
        &Method::DifferentialEvolution(opts),
        &Common {
            maxfev: Some(4),
            ..Common::default()
        },
    )
    .unwrap();
    assert_eq!(result.status, Termination::MaxEvaluations);
    assert_eq!(result.fun, -f64::MAX);
}

#[test]
fn random_initialization_repeats_for_zero_and_maximum_seed() {
    for seed in [0, u64::MAX] {
        let opts = DifferentialEvolutionOptions {
            global: GlobalOptions {
                seed,
                ..GlobalOptions::default()
            },
            ..DifferentialEvolutionOptions::default()
        };
        let first = run(
            opts.clone(),
            Common {
                maxiter: Some(2),
                ..Common::default()
            },
        );
        assert_eq!(
            first,
            run(
                opts,
                Common {
                    maxiter: Some(2),
                    ..Common::default()
                }
            )
        );
    }
}
