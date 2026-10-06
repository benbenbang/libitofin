use itofin_optimize::{
    Bounds, Common, DifferentialEvolutionOptions, GlobalOptions, InvalidInput, Method,
    MinimizeError, Objective, Problem, minimize,
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

fn invalid(p: &Problem, opts: DifferentialEvolutionOptions, common: Common) -> InvalidInput {
    let result = minimize(
        &mut |_: &[f64]| -> Result<f64, Infallible> { panic!("invalid input must never evaluate") },
        p,
        &Method::DifferentialEvolution(opts),
        &common,
    );
    match result {
        Err(MinimizeError::InvalidInput(error)) => error,
        other => panic!("{other:?}"),
    }
}

#[test]
fn public_option_validation_rejects_bad_problem_shape_without_panicking() {
    let mut p = problem();
    p.bounds.as_mut().unwrap().lower.clear();
    assert!(matches!(
        DifferentialEvolutionOptions::default().validate(&p),
        Err(InvalidInput::BoundsLength { .. })
    ));
    assert!(matches!(
        DifferentialEvolutionOptions::default().budgets(&Common {
            maxfev: Some(0),
            ..Common::default()
        }),
        Err(InvalidInput::NotPositive { option: "maxfev" })
    ));
}

#[test]
fn general_constraints_are_rejected_without_values_or_gradient_calls() {
    struct Constrained;
    impl Objective for Constrained {
        type Error = Infallible;
        fn value(&mut self, _: &[f64]) -> Result<f64, Self::Error> {
            panic!("unsupported")
        }
        fn constraint_count(&self) -> usize {
            1
        }
    }
    let result = minimize(
        &mut Constrained,
        &problem(),
        &Method::DifferentialEvolution(DifferentialEvolutionOptions::default()),
        &Common::default(),
    );
    assert!(matches!(
        result,
        Err(MinimizeError::InvalidInput(InvalidInput::Unsupported {
            method: "Differential-Evolution",
            option: "constraints"
        }))
    ));
}

#[test]
fn enormous_and_subnormal_boxes_never_produce_infeasible_nonfinite_points() {
    for (lower, upper, x0) in [
        (1e308, 1.7e308, 1.2e308),
        (-1e308, -0.1e308, -0.3e308),
        (-8e307, 8e307, 0.0),
        (0.0, f64::from_bits(10), f64::from_bits(4)),
    ] {
        let p = Problem {
            x0: vec![x0],
            bounds: Some(Bounds {
                lower: vec![lower],
                upper: vec![upper],
            }),
        };
        let opts = DifferentialEvolutionOptions {
            global: GlobalOptions {
                xatol: Some(0.0),
                fatol: Some(0.0),
                ..GlobalOptions::default()
            },
            mutation: 2.0,
            ..DifferentialEvolutionOptions::default()
        };
        let result = minimize(
            &mut |x: &[f64]| {
                assert!(x[0].is_finite() && x[0] >= lower && x[0] <= upper);
                Ok::<_, Infallible>(x[0] / lower.abs().max(upper.abs()).max(f64::MIN_POSITIVE))
            },
            &p,
            &Method::DifferentialEvolution(opts),
            &Common {
                maxiter: Some(5),
                ..Common::default()
            },
        )
        .unwrap();
        assert!(result.fun.is_finite());
        assert!(result.nfev <= 90);
    }
}

#[test]
fn every_population_row_is_validated_even_with_a_one_evaluation_budget() {
    let mut opts = DifferentialEvolutionOptions::default();
    opts.global.initial_population = Some(vec![vec![0.0], vec![0.0], vec![0.0], vec![3.0]]);
    assert_eq!(
        invalid(
            &problem(),
            opts,
            Common {
                maxfev: Some(1),
                ..Common::default()
            }
        ),
        InvalidInput::OutsideGlobalBounds {
            option: "initial_population",
            point: 3,
            index: 0
        }
    );
}

#[test]
fn supported_dimension_population_and_cell_endpoints_validate() {
    for dimension in [1, 256] {
        let p = Problem {
            x0: vec![0.0; dimension],
            bounds: Some(Bounds {
                lower: vec![-1.0; dimension],
                upper: vec![1.0; dimension],
            }),
        };
        for count in [4, 1_000_000 / dimension] {
            let opts = DifferentialEvolutionOptions {
                global: GlobalOptions {
                    population_size: Some(count.min(4096)),
                    ..GlobalOptions::default()
                },
                ..DifferentialEvolutionOptions::default()
            };
            assert!(opts.validate(&p).is_ok());
        }
    }
}

#[test]
fn zero_xatol_requires_identical_physical_points_even_when_normalization_rounds_them_equal() {
    let p = Problem {
        x0: vec![0.0],
        bounds: Some(Bounds {
            lower: vec![-1e100],
            upper: vec![1e100],
        }),
    };
    let opts = DifferentialEvolutionOptions {
        global: GlobalOptions {
            initial_population: Some(vec![vec![-2.0], vec![-1.0], vec![1.0], vec![2.0]]),
            xatol: Some(0.0),
            fatol: Some(0.0),
            ..GlobalOptions::default()
        },
        ..DifferentialEvolutionOptions::default()
    };
    let result = minimize(
        &mut |_: &[f64]| Ok::<_, Infallible>(-7.0),
        &p,
        &Method::DifferentialEvolution(opts),
        &Common {
            maxfev: Some(4),
            ..Common::default()
        },
    )
    .unwrap();
    assert_eq!(result.status, itofin_optimize::Termination::MaxEvaluations);
    assert_eq!((result.nit, result.nfev), (0, 4));
}

#[test]
fn zero_crossover_preserves_uncrossed_physical_coordinates_bit_exactly() {
    let points = vec![
        vec![0.0, 0.0, -0.0],
        vec![-1.0, -2.0, -0.0],
        vec![-3.0, -4.0, -0.0],
        vec![-5.0, -6.0, -0.0],
    ];
    let p = Problem {
        x0: points[0].clone(),
        bounds: Some(Bounds {
            lower: vec![-1e308, -1e308, -0.0],
            upper: vec![1.0, 1.0, 0.0],
        }),
    };
    let opts = DifferentialEvolutionOptions {
        global: GlobalOptions {
            initial_population: Some(points.clone()),
            xatol: Some(0.0),
            fatol: Some(0.0),
            ..GlobalOptions::default()
        },
        recombination: 0.0,
        ..DifferentialEvolutionOptions::default()
    };
    let mut calls = Vec::new();
    let result = minimize(
        &mut |x: &[f64]| {
            calls.push(x.to_vec());
            Ok::<_, Infallible>(-7.0)
        },
        &p,
        &Method::DifferentialEvolution(opts),
        &Common {
            maxiter: Some(1),
            ..Common::default()
        },
    )
    .unwrap();
    assert_eq!(calls[..4], points);
    assert_eq!((result.nit, result.nfev), (1, 8));
    for (target, trial) in points.iter().zip(&calls[4..]) {
        let changed = (0..2)
            .filter(|&index| target[index].to_bits() != trial[index].to_bits())
            .count();
        assert_eq!(changed, 1, "target={target:?}, trial={trial:?}");
        assert!(trial[..2].contains(&1.0));
        assert_eq!(trial[2].to_bits(), target[2].to_bits());
    }
}
