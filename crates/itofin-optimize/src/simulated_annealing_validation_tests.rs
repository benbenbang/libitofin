use super::*;
use crate::ConstraintKind;

#[test]
fn coefficient_domains_reject_invalid_and_nonfinite_values() {
    for (name, values) in [
        (
            "initial_temperature",
            vec![0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY],
        ),
        (
            "cooling_rate",
            vec![0.0, 1.0, -1.0, 2.0, f64::NAN, f64::INFINITY],
        ),
        ("step_size", vec![0.0, -1.0, 1.01, f64::NAN, f64::INFINITY]),
    ] {
        for value in values {
            let mut options = options();
            match name {
                "initial_temperature" => options.initial_temperature = value,
                "cooling_rate" => options.cooling_rate = value,
                "step_size" => options.step_size = value,
                _ => unreachable!(),
            }
            assert!(
                matches!(options.validate(&problem()), Err(InvalidInput::HybridSimulatedAnnealingCoefficient { option, .. }) if option == name)
            );
        }
    }
    for value in [f64::from_bits(1), 1e-300, f64::MAX] {
        let options = HybridSimulatedAnnealingOptions {
            initial_temperature: value,
            ..options()
        };
        assert!(options.validate(&problem()).is_ok());
    }
}

#[test]
fn bounded_integer_controls_and_tolerances_validate_before_evaluation() {
    for (name, cap) in [
        ("local_search_interval", 1_000_000),
        ("local_search_steps", 256),
        ("reanneal_interval", 1_000_000),
    ] {
        for value in [0, cap + 1, usize::MAX] {
            let mut options = options();
            match name {
                "local_search_interval" => options.local_search_interval = value,
                "local_search_steps" => options.local_search_steps = value,
                "reanneal_interval" => options.reanneal_interval = value,
                _ => unreachable!(),
            }
            assert!(
                matches!(options.validate(&problem()), Err(InvalidInput::GlobalRange { option, .. }) if option == name)
            );
        }
    }
    for name in ["xatol", "fatol"] {
        for value in [-1.0, f64::NAN, f64::INFINITY] {
            let mut options = options();
            if name == "xatol" {
                options.xatol = Some(value);
            } else {
                options.fatol = Some(value);
            }
            assert!(
                matches!(options.validate(&problem()), Err(InvalidInput::NotFiniteNonnegative { option }) if option == name)
            );
        }
    }
    assert!(options().validate(&problem()).is_ok());
}

#[test]
fn finite_box_start_dimension_and_width_are_mandatory() {
    let mut p = problem();
    p.bounds = None;
    assert_eq!(
        options().validate(&p),
        Err(InvalidInput::MissingGlobalBounds)
    );
    for (lower, upper) in [
        (f64::NEG_INFINITY, 2.0),
        (-2.0, f64::INFINITY),
        (-f64::MAX, f64::MAX),
    ] {
        let p = Problem {
            x0: vec![0.0],
            bounds: Some(Bounds {
                lower: vec![lower],
                upper: vec![upper],
            }),
        };
        assert_eq!(
            options().validate(&p),
            Err(InvalidInput::NonfiniteGlobalBound { index: 0 })
        );
    }
    let p = Problem {
        x0: vec![3.0],
        ..problem()
    };
    assert_eq!(
        options().validate(&p),
        Err(InvalidInput::OutsideGlobalBounds {
            option: "x0",
            point: 0,
            index: 0
        })
    );
    let p = Problem {
        x0: vec![0.0; 257],
        bounds: Some(Bounds {
            lower: vec![-1.0; 257],
            upper: vec![1.0; 257],
        }),
    };
    assert!(matches!(
        options().validate(&p),
        Err(InvalidInput::GlobalRange {
            option: "dimension",
            ..
        })
    ));
    let p = Problem {
        x0: vec![],
        bounds: None,
    };
    assert_eq!(options().validate(&p), Err(InvalidInput::EmptyX0));
}

#[test]
fn all_fixed_boxes_still_reject_bad_options_before_any_callback() {
    let fixed = Problem {
        x0: vec![1.0],
        bounds: Some(Bounds {
            lower: vec![1.0],
            upper: vec![1.0],
        }),
    };
    let mut objective = Recorder::default();
    let method = Method::HybridSimulatedAnnealing(HybridSimulatedAnnealingOptions {
        cooling_rate: 1.0,
        ..options()
    });
    assert!(matches!(
        crate::minimize(&mut objective, &fixed, &method, &Common::default()),
        Err(MinimizeError::InvalidInput(_))
    ));
    assert!(objective.points.is_empty());
}

#[test]
fn shared_global_budget_defaults_and_caps_are_preserved() {
    let options = options();
    let resolved = options.budgets(&Common::default()).unwrap();
    assert_eq!(
        (resolved.maxiter, resolved.maxfev),
        (Some(1000), Some(1_000_000))
    );
    for (name, common) in [
        (
            "maxiter",
            Common {
                maxiter: Some(1_000_001),
                ..Common::default()
            },
        ),
        (
            "maxfev",
            Common {
                maxfev: Some(10_000_001),
                ..Common::default()
            },
        ),
    ] {
        assert!(
            matches!(options.budgets(&common), Err(InvalidInput::GlobalRange { option, .. }) if option == name)
        );
    }
    assert!(
        options
            .budgets(&Common {
                maxiter: Some(0),
                ..Common::default()
            })
            .is_err()
    );
    assert!(
        options
            .budgets(&Common {
                maxfev: Some(0),
                ..Common::default()
            })
            .is_err()
    );
}

struct Constrained;

impl Objective for Constrained {
    type Error = io::Error;
    fn value(&mut self, _: &[f64]) -> Result<f64, io::Error> {
        panic!("unsupported constraints must be rejected first")
    }
    fn constraint_count(&self) -> usize {
        1
    }
    fn constraint_kind(&self, _: usize) -> ConstraintKind {
        ConstraintKind::Ineq
    }
}

#[test]
fn general_constraints_are_rejected_before_evaluating_an_objective() {
    let method = Method::HybridSimulatedAnnealing(options());
    assert!(method.supports_bounds());
    assert_eq!(method.name(), "Hybrid-Simulated-Annealing");
    assert!(matches!(
        crate::minimize(&mut Constrained, &problem(), &method, &Common::default()),
        Err(MinimizeError::InvalidInput(InvalidInput::Unsupported {
            method: "Hybrid-Simulated-Annealing",
            option: "constraints"
        }))
    ));
}
