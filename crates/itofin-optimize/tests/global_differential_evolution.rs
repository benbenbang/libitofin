use itofin_optimize::{
    Bounds, Common, DifferentialEvolutionOptions, Flow, GlobalOptions, IterationState, Method,
    Minimize, Objective, Problem, Termination, minimize,
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

#[test]
fn bounded_negative_quadratic_converges_to_interior_solution() {
    let result = run(DifferentialEvolutionOptions::default(), Common::default());
    assert!(result.success, "{result:?}");
    assert!((result.x[0] - 0.37).abs() <= 1e-5);
    assert!((result.fun + 3.0).abs() <= 1e-10);
    assert_eq!(result.njev, 0);
    assert_eq!(result.nfev, 15 * (result.nit + 1));
}

#[test]
fn box_boundary_solution_and_fixed_coordinate_remain_feasible() {
    let p = Problem {
        x0: vec![0.0, 7.0],
        bounds: Some(Bounds {
            lower: vec![-2.0, 7.0],
            upper: vec![2.0, 7.0],
        }),
    };
    let result = minimize(
        &mut |x: &[f64]| {
            assert_eq!(x[1], 7.0);
            assert!((-2.0..=2.0).contains(&x[0]));
            Ok::<_, Infallible>((x[0] - 5.0).powi(2))
        },
        &p,
        &Method::DifferentialEvolution(DifferentialEvolutionOptions::default()),
        &Common::default(),
    )
    .unwrap();
    assert!(result.success, "{result:?}");
    assert!((result.x[0] - 2.0).abs() <= 1e-5);
    assert_eq!(result.x[1], 7.0);
    assert_eq!(result.nfev, 15 * (result.nit + 1));
}

#[test]
fn deterministic_seed_including_zero_repeats_the_entire_result() {
    let first = run(
        options(),
        Common {
            maxiter: Some(3),
            ..Common::default()
        },
    );
    assert_eq!(
        first,
        run(
            options(),
            Common {
                maxiter: Some(3),
                ..Common::default()
            }
        )
    );
    let mut different = options();
    different.global.seed = 42;
    assert_ne!(
        first,
        run(
            different,
            Common {
                maxiter: Some(3),
                ..Common::default()
            }
        )
    );
}

#[test]
fn explicit_population_is_evaluated_unchanged_and_not_replaced_by_x0() {
    let p = problem();
    let given = vec![vec![-2.0], vec![-1.0000000000000002], vec![1.0], vec![2.0]];
    let mut opts = options();
    opts.global.initial_population = Some(given.clone());
    let mut calls = Vec::new();
    let result = minimize(
        &mut |x: &[f64]| {
            calls.push(x.to_vec());
            Ok::<_, Infallible>(x[0] * x[0])
        },
        &p,
        &Method::DifferentialEvolution(opts),
        &Common {
            maxfev: Some(4),
            ..Common::default()
        },
    )
    .unwrap();
    assert_eq!(calls, given);
    assert_eq!(result.nfev, 4);
    assert_eq!(result.nit, 0);
    assert_eq!(result.status, Termination::MaxEvaluations);
}

#[test]
fn random_population_starts_with_exact_x0_and_free_dimension_default_size() {
    let p = Problem {
        x0: vec![0.123, -0.0],
        bounds: Some(Bounds {
            lower: vec![-2.0, -0.0],
            upper: vec![2.0, 0.0],
        }),
    };
    let mut calls = Vec::new();
    let result = minimize(
        &mut |x: &[f64]| {
            calls.push(x.to_vec());
            Ok::<_, Infallible>(x[0])
        },
        &p,
        &Method::DifferentialEvolution(DifferentialEvolutionOptions::default()),
        &Common {
            maxfev: Some(15),
            ..Common::default()
        },
    )
    .unwrap();
    assert_eq!(calls[0][0].to_bits(), p.x0[0].to_bits());
    assert_eq!(calls[0][1].to_bits(), p.x0[1].to_bits());
    assert_eq!(result.nfev, 15);
    assert_eq!(result.nit, 0);
}

#[test]
fn all_fixed_evaluates_exact_x0_once_without_callback_or_gradient() {
    struct Fixed;
    impl Objective for Fixed {
        type Error = Infallible;
        fn value(&mut self, x: &[f64]) -> Result<f64, Self::Error> {
            assert_eq!(x[0].to_bits(), (-0.0f64).to_bits());
            Ok(-7.0)
        }
        fn gradient(&mut self, _: &[f64], _: &mut [f64]) -> Result<bool, Self::Error> {
            panic!("DE must not request gradients")
        }
        fn callback(&mut self, _: &IterationState<'_>) -> Result<Flow, Self::Error> {
            panic!("no complete generation")
        }
    }
    let p = Problem {
        x0: vec![-0.0],
        bounds: Some(Bounds {
            lower: vec![0.0],
            upper: vec![0.0],
        }),
    };
    let result = minimize(
        &mut Fixed,
        &p,
        &Method::DifferentialEvolution(DifferentialEvolutionOptions::default()),
        &Common {
            maxfev: Some(1),
            ..Common::default()
        },
    )
    .unwrap();
    assert!(result.success);
    assert_eq!(
        (result.fun, result.nit, result.nfev, result.njev),
        (-7.0, 0, 1, 0)
    );
}

#[test]
fn partial_initialization_and_generation_return_best_ever_without_iterations() {
    for budget in 1..8 {
        let result = run(
            options(),
            Common {
                maxfev: Some(budget),
                ..Common::default()
            },
        );
        assert_eq!(result.nfev, budget);
        assert_eq!(result.nit, 0);
        assert_eq!(result.status, Termination::MaxEvaluations);
        assert_eq!(result.fun, (result.x[0] - 0.37).powi(2) - 3.0);
        if budget >= 3 {
            assert!(result.fun <= (1.0f64 - 0.37).powi(2) - 3.0);
        }
    }
    let result = run(
        options(),
        Common {
            maxiter: Some(2),
            ..Common::default()
        },
    );
    assert_eq!((result.nit, result.nfev), (2, 12));
    assert_eq!(result.status, Termination::MaxIterations);
}
