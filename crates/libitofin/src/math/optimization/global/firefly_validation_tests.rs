use super::*;

#[test]
fn constructor_rejects_invalid_bounds_coefficients_populations_and_budgets() {
    for upper in [f64::INFINITY, f64::NAN, -1.0] {
        assert!(
            Firefly::new(
                Bounds {
                    lower: vec![0.0],
                    upper: vec![upper]
                },
                FireflyOptions::default(),
                Common::default()
            )
            .is_err()
        );
    }
    for options in [
        FireflyOptions {
            alpha: -0.1,
            ..FireflyOptions::default()
        },
        FireflyOptions {
            beta0: 0.0,
            ..FireflyOptions::default()
        },
        FireflyOptions {
            gamma: 1_000_001.0,
            ..FireflyOptions::default()
        },
        FireflyOptions {
            alpha_decay: 0.0,
            ..FireflyOptions::default()
        },
        FireflyOptions {
            global: GlobalOptions {
                population_size: Some(3),
                ..GlobalOptions::default()
            },
            ..FireflyOptions::default()
        },
    ] {
        assert!(
            Firefly::new(
                Bounds {
                    lower: vec![0.0],
                    upper: vec![1.0]
                },
                options,
                Common::default()
            )
            .is_err()
        );
    }
    assert!(
        Firefly::new(
            Bounds {
                lower: vec![0.0],
                upper: vec![1.0]
            },
            FireflyOptions::default(),
            Common {
                maxfev: Some(10_000_001),
                ..Common::default()
            }
        )
        .is_err()
    );
}

#[test]
fn empty_intersection_fails_before_problem_reset_or_pricing() {
    let cost = SignedCost(Cell::new(0));
    let constraint = BoundaryConstraint::new(2.0, 3.0);
    let mut problem = Problem::new(&cost, &constraint, Array::from([2.5]));
    problem.set_function_value(17.0);
    let mut method = solver(Common::default());
    let error = method.minimize(&mut problem, &criteria()).unwrap_err();
    assert!(error.message().contains("empty constraint intersection"));
    assert_eq!(problem.function_value(), 17.0);
    assert_eq!(problem.function_evaluation(), 0);
    assert_eq!(cost.0.get(), 0);
    assert!(method.last_result().is_none());
}
