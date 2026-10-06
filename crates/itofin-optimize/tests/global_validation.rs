use itofin_optimize::{
    Bounds, Common, DifferentialEvolutionOptions, GlobalOptions, InvalidInput, Method,
    MinimizeError, Problem, minimize,
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
fn defaults_method_and_bounded_budget_contract_are_explicit() {
    let opts = DifferentialEvolutionOptions::default();
    assert_eq!(
        opts.global,
        GlobalOptions {
            seed: 0,
            population_size: None,
            initial_population: None,
            xatol: None,
            fatol: None
        }
    );
    assert_eq!((opts.mutation, opts.recombination), (0.8, 0.9));
    let method = Method::DifferentialEvolution(opts.clone());
    assert_eq!(method.name(), "Differential-Evolution");
    assert!(method.supports_bounds());
    let budgets = opts.budgets(&Common::default()).unwrap();
    assert_eq!(
        (budgets.maxiter, budgets.maxfev),
        (Some(1000), Some(1_000_000))
    );
    assert_eq!(
        opts.budgets(&Common {
            maxiter: Some(2),
            ..Common::default()
        })
        .unwrap()
        .maxfev,
        Some(1_000_000)
    );
    assert_eq!(
        opts.budgets(&Common {
            maxfev: Some(2),
            ..Common::default()
        })
        .unwrap()
        .maxiter,
        Some(1000)
    );
}

#[test]
fn missing_unbounded_overflow_width_and_outside_x0_reject_without_evaluation() {
    let mut p = problem();
    p.bounds = None;
    assert_eq!(
        invalid(
            &p,
            DifferentialEvolutionOptions::default(),
            Common::default()
        ),
        InvalidInput::MissingGlobalBounds
    );
    for (lower, upper) in [
        (f64::NEG_INFINITY, 2.0),
        (-2.0, f64::INFINITY),
        (-f64::MAX, f64::MAX),
    ] {
        p.bounds = Some(Bounds {
            lower: vec![lower],
            upper: vec![upper],
        });
        assert_eq!(
            invalid(
                &p,
                DifferentialEvolutionOptions::default(),
                Common::default()
            ),
            InvalidInput::NonfiniteGlobalBound { index: 0 }
        );
    }
    p = problem();
    p.x0[0] = 3.0;
    assert_eq!(
        invalid(
            &p,
            DifferentialEvolutionOptions::default(),
            Common::default()
        ),
        InvalidInput::OutsideGlobalBounds {
            option: "x0",
            point: 0,
            index: 0
        }
    );
}

#[test]
fn dimensions_population_cells_and_work_caps_are_checked_before_evaluation() {
    let p = Problem {
        x0: vec![0.0; 257],
        bounds: Some(Bounds {
            lower: vec![-1.0; 257],
            upper: vec![1.0; 257],
        }),
    };
    assert!(matches!(
        invalid(
            &p,
            DifferentialEvolutionOptions::default(),
            Common::default()
        ),
        InvalidInput::GlobalRange {
            option: "dimension",
            found: 257,
            ..
        }
    ));
    for size in [0, 1, 3, 4097, usize::MAX] {
        let mut opts = DifferentialEvolutionOptions::default();
        opts.global.population_size = Some(size);
        assert!(matches!(
            invalid(&problem(), opts, Common::default()),
            InvalidInput::GlobalRange {
                option: "population_size",
                ..
            }
        ));
    }
    let p = Problem {
        x0: vec![0.0; 256],
        bounds: Some(Bounds {
            lower: vec![-1.0; 256],
            upper: vec![1.0; 256],
        }),
    };
    let mut opts = DifferentialEvolutionOptions::default();
    opts.global.population_size = Some(4096);
    assert_eq!(
        invalid(&p, opts, Common::default()),
        InvalidInput::PopulationCells { max: 1_000_000 }
    );
    for common in [
        Common {
            maxiter: Some(1_000_001),
            ..Common::default()
        },
        Common {
            maxfev: Some(10_000_001),
            ..Common::default()
        },
    ] {
        assert!(matches!(
            invalid(&problem(), DifferentialEvolutionOptions::default(), common),
            InvalidInput::GlobalRange { .. }
        ));
    }
}

#[test]
fn malformed_population_rejects_all_shape_and_coordinate_errors() {
    let valid = vec![vec![-2.0], vec![-1.0], vec![1.0], vec![2.0]];
    let mut opts = DifferentialEvolutionOptions::default();
    opts.global.initial_population = Some(valid.clone());
    opts.global.population_size = Some(5);
    assert_eq!(
        invalid(&problem(), opts.clone(), Common::default()),
        InvalidInput::PopulationPointCount {
            expected: 5,
            found: 4
        }
    );
    opts.global.population_size = None;
    opts.global.initial_population.as_mut().unwrap()[2].push(1.0);
    assert_eq!(
        invalid(&problem(), opts.clone(), Common::default()),
        InvalidInput::PopulationPointLength {
            point: 2,
            expected: 1,
            found: 2
        }
    );
    opts.global.initial_population = Some(valid);
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        opts.global.initial_population.as_mut().unwrap()[2][0] = value;
        assert_eq!(
            invalid(&problem(), opts.clone(), Common::default()),
            InvalidInput::NonfinitePopulation { point: 2, index: 0 }
        );
    }
    opts.global.initial_population.as_mut().unwrap()[2][0] = 3.0;
    assert_eq!(
        invalid(&problem(), opts, Common::default()),
        InvalidInput::OutsideGlobalBounds {
            option: "initial_population",
            point: 2,
            index: 0
        }
    );
}

#[test]
fn tolerance_and_mutation_crossover_ranges_are_validated_without_evaluation() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
        for is_x in [true, false] {
            let mut opts = DifferentialEvolutionOptions::default();
            if is_x {
                opts.global.xatol = Some(value);
            } else {
                opts.global.fatol = Some(value);
            }
            assert!(matches!(
                invalid(&problem(), opts, Common::default()),
                InvalidInput::NotFiniteNonnegative { .. }
            ));
        }
    }
    for value in [f64::NAN, f64::INFINITY, -1.0, 0.0, 2.1] {
        let opts = DifferentialEvolutionOptions {
            mutation: value,
            ..DifferentialEvolutionOptions::default()
        };
        assert!(matches!(
            invalid(&problem(), opts, Common::default()),
            InvalidInput::DifferentialEvolutionCoefficient {
                option: "mutation",
                ..
            }
        ));
    }
    for value in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        let opts = DifferentialEvolutionOptions {
            recombination: value,
            ..DifferentialEvolutionOptions::default()
        };
        assert!(matches!(
            invalid(&problem(), opts, Common::default()),
            InvalidInput::DifferentialEvolutionCoefficient {
                option: "recombination",
                ..
            }
        ));
    }
    for mutation in [f64::MIN_POSITIVE, 2.0] {
        for recombination in [0.0, 1.0] {
            let opts = DifferentialEvolutionOptions {
                mutation,
                recombination,
                ..DifferentialEvolutionOptions::default()
            };
            assert!(opts.validate(&problem()).is_ok());
        }
    }
}
