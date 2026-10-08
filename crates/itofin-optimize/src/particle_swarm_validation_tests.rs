use super::*;

fn rejected(p: &Problem, opts: ParticleSwarmOptions, common: Common) -> InvalidInput {
    let mut objective = Recorder::default();
    let error =
        crate::minimize(&mut objective, p, &Method::ParticleSwarm(opts), &common).unwrap_err();
    assert!(objective.points.is_empty());
    match error {
        MinimizeError::InvalidInput(error) => error,
        other => panic!("{other:?}"),
    }
}

#[test]
fn method_identity_and_defaults_are_explicit() {
    let options = ParticleSwarmOptions::default();
    assert_eq!(
        (
            options.inertia,
            options.cognitive,
            options.social,
            options.velocity_clamp
        ),
        (0.7, 1.4, 1.4, 0.2)
    );
    let method = Method::ParticleSwarm(options.clone());
    assert_eq!(method.name(), "Particle-Swarm");
    assert!(method.supports_bounds());
    let budgets = options.budgets(&Common::default()).unwrap();
    assert_eq!(
        (budgets.maxiter, budgets.maxfev),
        (Some(1000), Some(1_000_000))
    );
}

#[test]
fn coefficients_validate_finite_ranges_before_any_evaluation() {
    for (name, range, valid, invalid) in [
        (
            "inertia",
            "[0, 1]",
            vec![0.0, 1.0],
            vec![-0.1, 1.1, f64::NAN, f64::INFINITY],
        ),
        (
            "cognitive",
            "[0, 4]",
            vec![0.0, 4.0],
            vec![-0.1, 4.1, f64::NAN, f64::INFINITY],
        ),
        (
            "social",
            "[0, 4]",
            vec![0.0, 4.0],
            vec![-0.1, 4.1, f64::NAN, f64::NEG_INFINITY],
        ),
        (
            "velocity_clamp",
            "(0, 1]",
            vec![f64::from_bits(1), 1.0],
            vec![0.0, -0.1, 1.1, f64::NAN, f64::INFINITY],
        ),
    ] {
        for value in valid.into_iter().chain(invalid.iter().copied()) {
            let mut opts = options();
            match name {
                "inertia" => opts.inertia = value,
                "cognitive" => opts.cognitive = value,
                "social" => opts.social = value,
                _ => opts.velocity_clamp = value,
            }
            if invalid.iter().any(|bad| bad.to_bits() == value.to_bits()) {
                assert_eq!(
                    rejected(&problem(), opts, Common::default()),
                    InvalidInput::ParticleSwarmCoefficient {
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
fn bounds_are_required_finite_ordered_and_x0_feasible() {
    let mut p = problem();
    p.bounds = None;
    assert_eq!(
        rejected(&p, options(), Common::default()),
        InvalidInput::MissingGlobalBounds
    );
    for (lower, upper) in [
        (f64::NEG_INFINITY, 2.0),
        (-2.0, f64::INFINITY),
        (-1e308, 1e308),
    ] {
        let mut p = problem();
        p.bounds = Some(Bounds {
            lower: vec![lower],
            upper: vec![upper],
        });
        assert_eq!(
            rejected(&p, options(), Common::default()),
            InvalidInput::NonfiniteGlobalBound { index: 0 }
        );
    }
    let mut p = problem();
    p.x0 = vec![3.0];
    assert!(matches!(
        rejected(&p, options(), Common::default()),
        InvalidInput::OutsideGlobalBounds { option: "x0", .. }
    ));
    let mut p = problem();
    p.bounds.as_mut().unwrap().lower = vec![3.0];
    assert_eq!(
        rejected(&p, options(), Common::default()),
        InvalidInput::BoundsOrder { index: 0 }
    );
}

#[test]
fn malformed_population_options_are_rejected_before_evaluation() {
    let mut opts = options();
    opts.global.population_size = Some(3);
    assert!(matches!(
        rejected(&problem(), opts, Common::default()),
        InvalidInput::GlobalRange {
            option: "population_size",
            ..
        }
    ));
    let mut opts = options();
    opts.global.population_size = Some(4097);
    assert!(matches!(
        rejected(&problem(), opts, Common::default()),
        InvalidInput::GlobalRange {
            option: "population_size",
            ..
        }
    ));
    let mut opts = options();
    opts.global.initial_population.as_mut().unwrap().pop();
    opts.global.population_size = Some(4);
    assert!(matches!(
        rejected(&problem(), opts, Common::default()),
        InvalidInput::PopulationPointCount { .. }
    ));
    for bad in [vec![], vec![0.0, 1.0], vec![f64::NAN], vec![3.0]] {
        let mut opts = options();
        opts.global.initial_population.as_mut().unwrap()[2] = bad;
        rejected(&problem(), opts, Common::default());
    }
}

#[test]
fn dimensions_population_cells_tolerances_and_budgets_have_caps() {
    let mut p = Problem {
        x0: vec![0.0; 257],
        bounds: Some(Bounds {
            lower: vec![-1.0; 257],
            upper: vec![1.0; 257],
        }),
    };
    assert!(matches!(
        rejected(&p, ParticleSwarmOptions::default(), Common::default()),
        InvalidInput::GlobalRange {
            option: "dimension",
            ..
        }
    ));
    p.x0.pop();
    p.bounds.as_mut().unwrap().lower.pop();
    p.bounds.as_mut().unwrap().upper.pop();
    let mut opts = ParticleSwarmOptions::default();
    opts.global.population_size = Some(4096);
    assert!(matches!(
        rejected(&p, opts, Common::default()),
        InvalidInput::PopulationCells { .. }
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

#[test]
fn unsupported_constraints_are_rejected_without_calls() {
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
        &Method::ParticleSwarm(options()),
        &Common::default(),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        MinimizeError::InvalidInput(InvalidInput::Unsupported {
            method: "Particle-Swarm",
            option: "constraints"
        })
    ));
}
