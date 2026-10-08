use super::*;

fn rejected(p: &Problem, options: FireflyOptions, common: Common) -> InvalidInput {
    let mut objective = Recorder::default();
    let error = crate::minimize(&mut objective, p, &Method::Firefly(options), &common).unwrap_err();
    assert!(objective.points.is_empty());
    match error {
        MinimizeError::InvalidInput(error) => error,
        other => panic!("{other:?}"),
    }
}

#[test]
fn method_name_defaults_and_budgets_are_explicit() {
    let opts = FireflyOptions::default();
    assert_eq!(
        (opts.alpha, opts.beta0, opts.gamma, opts.alpha_decay),
        (0.25, 1.0, 1.0, 0.97)
    );
    let method = Method::Firefly(opts.clone());
    assert_eq!(method.name(), "Firefly");
    assert!(method.supports_bounds());
    let budgets = opts.budgets(&Common::default()).unwrap();
    assert_eq!(
        (budgets.maxiter, budgets.maxfev),
        (Some(1000), Some(1_000_000))
    );
}

#[test]
fn coefficients_reject_nonfinite_and_out_of_range_values_before_calls() {
    for (name, range, valid, invalid) in [
        (
            "alpha",
            "[0, 1]",
            vec![0.0, 1.0],
            vec![-0.1, 1.1, f64::NAN, f64::INFINITY],
        ),
        (
            "beta0",
            "(0, 1]",
            vec![f64::from_bits(1), 1.0],
            vec![0.0, -0.1, 1.1, f64::NAN],
        ),
        (
            "gamma",
            "[0, 1e6]",
            vec![0.0, 1e6],
            vec![-0.1, 1e6 + 1.0, f64::NAN, f64::INFINITY],
        ),
        (
            "alpha_decay",
            "(0, 1]",
            vec![f64::from_bits(1), 1.0],
            vec![0.0, -0.1, 1.1, f64::NAN],
        ),
    ] {
        for value in valid.into_iter().chain(invalid.iter().copied()) {
            let mut opts = options();
            match name {
                "alpha" => opts.alpha = value,
                "beta0" => opts.beta0 = value,
                "gamma" => opts.gamma = value,
                _ => opts.alpha_decay = value,
            }
            if invalid.iter().any(|bad| bad.to_bits() == value.to_bits()) {
                assert_eq!(
                    rejected(&problem(), opts, Common::default()),
                    InvalidInput::FireflyCoefficient {
                        option: name,
                        range
                    }
                );
            } else {
                opts.validate(&problem()).unwrap();
            }
        }
    }
}

#[test]
fn invalid_bounds_population_shapes_and_fixed_box_coefficients_reject_without_calls() {
    let mut p = problem();
    p.bounds = None;
    assert_eq!(
        rejected(&p, options(), Common::default()),
        InvalidInput::MissingGlobalBounds
    );
    for (lo, hi) in [
        (f64::NEG_INFINITY, 2.0),
        (-2.0, f64::INFINITY),
        (-1e308, 1e308),
    ] {
        let p = Problem {
            x0: vec![0.0],
            bounds: Some(Bounds {
                lower: vec![lo],
                upper: vec![hi],
            }),
        };
        assert_eq!(
            rejected(&p, options(), Common::default()),
            InvalidInput::NonfiniteGlobalBound { index: 0 }
        );
    }
    for bad in [vec![], vec![0.0, 1.0], vec![f64::NAN], vec![3.0]] {
        let mut opts = options();
        opts.global.initial_population.as_mut().unwrap()[2] = bad;
        rejected(&problem(), opts, Common::default());
    }
    let p = Problem {
        x0: vec![7.0],
        bounds: Some(Bounds {
            lower: vec![7.0],
            upper: vec![7.0],
        }),
    };
    assert_eq!(
        rejected(
            &p,
            FireflyOptions {
                alpha: -1.0,
                ..FireflyOptions::default()
            },
            Common::default()
        ),
        InvalidInput::FireflyCoefficient {
            option: "alpha",
            range: "[0, 1]"
        }
    );
}

#[test]
fn population_dimension_tolerances_and_work_caps_reject_without_calls() {
    for size in [3, 4097] {
        let mut opts = options();
        opts.global.population_size = Some(size);
        assert!(matches!(
            rejected(&problem(), opts, Common::default()),
            InvalidInput::GlobalRange {
                option: "population_size",
                ..
            }
        ));
    }
    let p = Problem {
        x0: vec![0.0; 257],
        bounds: Some(Bounds {
            lower: vec![-1.0; 257],
            upper: vec![1.0; 257],
        }),
    };
    assert!(matches!(
        rejected(&p, FireflyOptions::default(), Common::default()),
        InvalidInput::GlobalRange {
            option: "dimension",
            ..
        }
    ));
    for value in [-1.0, f64::NAN, f64::INFINITY] {
        for name in ["xatol", "fatol"] {
            let mut opts = options();
            if name == "xatol" {
                opts.global.xatol = Some(value);
            } else {
                opts.global.fatol = Some(value);
            }
            assert_eq!(
                rejected(&problem(), opts, Common::default()),
                InvalidInput::NotFiniteNonnegative { option: name }
            );
        }
    }
    for (maxiter, maxfev) in [
        (Some(0), None),
        (None, Some(0)),
        (Some(1_000_001), None),
        (None, Some(10_000_001)),
    ] {
        rejected(
            &problem(),
            options(),
            Common {
                maxiter,
                maxfev,
                tol: None,
            },
        );
    }
}

#[test]
fn constraints_are_unsupported_without_value_or_gradient_calls() {
    struct Constrained;
    impl Objective for Constrained {
        type Error = io::Error;
        fn value(&mut self, _: &[f64]) -> Result<f64, Self::Error> {
            panic!("unsupported")
        }
        fn constraint_count(&self) -> usize {
            1
        }
    }
    let error = crate::minimize(
        &mut Constrained,
        &problem(),
        &Method::Firefly(options()),
        &Common::default(),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        MinimizeError::InvalidInput(InvalidInput::Unsupported {
            method: "Firefly",
            option: "constraints"
        })
    ));
}

#[test]
fn total_population_cells_and_mismatched_count_are_checked() {
    let p = Problem {
        x0: vec![0.0; 256],
        bounds: Some(Bounds {
            lower: vec![-1.0; 256],
            upper: vec![1.0; 256],
        }),
    };
    let mut opts = FireflyOptions::default();
    opts.global.population_size = Some(4096);
    assert!(matches!(
        rejected(&p, opts, Common::default()),
        InvalidInput::PopulationCells { .. }
    ));
    let mut opts = options();
    opts.global.population_size = Some(5);
    assert_eq!(
        rejected(&problem(), opts, Common::default()),
        InvalidInput::PopulationPointCount {
            expected: 5,
            found: 4
        }
    );
    let mut p = problem();
    p.x0[0] = 3.0;
    assert!(matches!(
        rejected(&p, options(), Common::default()),
        InvalidInput::OutsideGlobalBounds { option: "x0", .. }
    ));
    assert_eq!(
        rejected(
            &problem(),
            options(),
            Common {
                tol: Some(f64::INFINITY),
                ..Common::default()
            }
        ),
        InvalidInput::NotFinitePositive { option: "tol" }
    );
}
