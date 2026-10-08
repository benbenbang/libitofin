use super::*;
use crate::math::optimization::constraint::{BoundaryConstraint, Constraint, NoConstraint};
use crate::math::optimization::costfunction::CostFunction;
use itofin_optimize::GlobalOptions;
use std::cell::Cell;

struct SignedCost(Cell<usize>);
impl CostFunction for SignedCost {
    fn values(&self, _: &Array) -> Array {
        panic!("firefly must honor signed scalar overrides")
    }
    fn value(&self, x: &Array) -> f64 {
        self.0.set(self.0.get() + 1);
        (x[0] - 0.25).powi(2) - 5.0
    }
    fn gradient(&self, _: &mut Array, _: &Array) {
        panic!("firefly must not price gradients")
    }
}
fn criteria() -> EndCriteria {
    EndCriteria::new(1000, Some(10), 1e-9, 1e-9, None).unwrap()
}
fn solver(common: Common) -> Firefly {
    Firefly::new(
        Bounds {
            lower: vec![-1.0],
            upper: vec![1.0],
        },
        FireflyOptions::default(),
        common,
    )
    .unwrap()
}

#[test]
fn signed_solution_and_counts_match_actual_scalar_pricing() {
    let cost = SignedCost(Cell::new(0));
    let mut problem = Problem::new(&cost, &NoConstraint, Array::from([0.0]));
    let mut method = solver(Common::default());
    assert!(
        method
            .minimize(&mut problem, &criteria())
            .unwrap()
            .succeeded()
    );
    let result = method.last_result().unwrap();
    assert!(result.success);
    assert!((result.x[0] - 0.25).abs() < 1e-4);
    assert!((result.fun + 5.0).abs() < 1e-8);
    assert_eq!(result.x, problem.current_value().to_vec());
    assert_eq!(result.fun, problem.function_value());
    assert_eq!(result.nfev, cost.0.get());
    assert_eq!(result.nfev, problem.function_evaluation() as usize);
    assert_eq!(result.njev, 0);
    assert_eq!(problem.gradient_evaluation(), 0);
    assert_eq!(method.global_result(), method.last_result());
}

#[test]
fn seeded_repeated_run_preserves_exact_diagnostics() {
    let cost = SignedCost(Cell::new(0));
    let mut first = Problem::new(&cost, &NoConstraint, Array::from([0.0]));
    let mut method = solver(Common::default());
    method.minimize(&mut first, &criteria()).unwrap();
    let result = method.last_result().unwrap().clone();
    let mut second = Problem::new(&cost, &NoConstraint, Array::from([0.0]));
    method.minimize(&mut second, &criteria()).unwrap();
    assert_eq!(*method.last_result().unwrap(), result);
    assert_eq!(cost.0.get(), 2 * result.nfev);
}

#[test]
fn partial_population_budget_counts_only_real_evaluations() {
    for maxfev in [1, 3] {
        let cost = SignedCost(Cell::new(0));
        let mut problem = Problem::new(&cost, &NoConstraint, Array::from([0.0]));
        let mut method = solver(Common {
            maxfev: Some(maxfev),
            ..Common::default()
        });
        assert_eq!(
            method.minimize(&mut problem, &criteria()).unwrap(),
            EndCriteriaType::Unknown
        );
        let result = method.last_result().unwrap();
        assert_eq!(
            (result.status, result.nit, result.nfev),
            (Termination::MaxEvaluations, 0, maxfev)
        );
        assert!(!result.success);
        assert_eq!(cost.0.get(), maxfev);
        assert_eq!(problem.function_evaluation() as usize, maxfev);
    }
}

#[test]
fn completed_generation_limit_accounts_each_brightness_move_without_extra_pricing() {
    let cost = SignedCost(Cell::new(0));
    let mut problem = Problem::new(&cost, &NoConstraint, Array::from([0.0]));
    let mut method = Firefly::new(
        Bounds {
            lower: vec![-1.0],
            upper: vec![1.0],
        },
        FireflyOptions {
            global: GlobalOptions {
                initial_population: Some(vec![vec![-1.0], vec![0.0], vec![0.5], vec![1.0]]),
                ..GlobalOptions::default()
            },
            ..FireflyOptions::default()
        },
        Common {
            maxiter: Some(1),
            ..Common::default()
        },
    )
    .unwrap();
    assert_eq!(
        method.minimize(&mut problem, &criteria()).unwrap(),
        EndCriteriaType::MaxIterations
    );
    let result = method.last_result().unwrap();
    assert_eq!((result.status, result.nit), (Termination::MaxIterations, 1));
    assert!(!result.success);
    assert!(result.nfev > 4);
    assert_eq!(result.nfev, cost.0.get());
    assert_eq!(result.nfev, problem.function_evaluation() as usize);
}

#[test]
fn end_criteria_caps_search_iterations() {
    let cost = SignedCost(Cell::new(0));
    let mut problem = Problem::new(&cost, &NoConstraint, Array::from([0.0]));
    let mut method = solver(Common {
        maxiter: Some(100),
        ..Common::default()
    });
    let criteria = EndCriteria::new(3, Some(2), 0.0, 0.0, None).unwrap();
    assert_eq!(
        method.minimize(&mut problem, &criteria).unwrap(),
        EndCriteriaType::MaxIterations
    );
    let result = method.last_result().unwrap();
    assert_eq!((result.status, result.nit), (Termination::MaxIterations, 3));
    assert_eq!(result.nfev, cost.0.get());
}

#[test]
fn bounds_intersection_clamps_start_without_pricing_outside_the_constraint() {
    let cost = SignedCost(Cell::new(0));
    let constraint = BoundaryConstraint::new(-0.1, 0.1);
    let mut problem = Problem::new(&cost, &constraint, Array::from([0.0]));
    let mut method = Firefly::new(
        Bounds {
            lower: vec![0.05],
            upper: vec![1.0],
        },
        FireflyOptions::default(),
        Common::default(),
    )
    .unwrap();
    method.minimize(&mut problem, &criteria()).unwrap();
    let result = method.last_result().unwrap();
    assert!((0.05..=0.1).contains(&result.x[0]));
    assert!((result.x[0] - 0.1).abs() < 1e-4);
    assert_eq!(result.nfev, cost.0.get());
}

#[test]
fn explicit_population_outside_constraint_intersection_fails_before_reset() {
    let cost = SignedCost(Cell::new(0));
    let constraint = BoundaryConstraint::new(-0.1, 0.1);
    let mut problem = Problem::new(&cost, &constraint, Array::from([0.0]));
    problem.set_function_value(17.0);
    let mut method = Firefly::new(
        Bounds {
            lower: vec![-1.0],
            upper: vec![1.0],
        },
        FireflyOptions {
            global: GlobalOptions {
                initial_population: Some(vec![vec![0.0], vec![0.05], vec![0.2], vec![0.3]]),
                ..GlobalOptions::default()
            },
            ..FireflyOptions::default()
        },
        Common::default(),
    )
    .unwrap();
    assert!(method.minimize(&mut problem, &criteria()).is_err());
    assert_eq!(cost.0.get(), 0);
    assert_eq!(problem.function_evaluation(), 0);
    assert_eq!(problem.function_value(), 17.0);
    assert!(method.last_result().is_none());
}

#[test]
fn coupled_candidate_rejection_precedes_scalar_pricing() {
    struct OnlyOrigin;
    impl Constraint for OnlyOrigin {
        fn test(&self, x: &Array) -> bool {
            x[0] == 0.0
        }
    }
    let cost = SignedCost(Cell::new(0));
    let mut problem = Problem::new(&cost, &OnlyOrigin, Array::from([0.0]));
    let mut method = solver(Common::default());
    let error = method.minimize(&mut problem, &criteria()).unwrap_err();
    assert!(error.message().contains("firefly candidate"));
    assert!(error.message().contains("wholly feasible box"));
    assert_eq!(cost.0.get(), 1);
    assert_eq!(problem.function_evaluation(), 1);
    assert!(method.last_result().is_none());
    assert_eq!(problem.current_value(), &Array::from([0.0]));
}

#[test]
fn nonfinite_scalar_returns_error_with_retained_termination_diagnostic() {
    struct Nonfinite;
    impl CostFunction for Nonfinite {
        fn values(&self, _: &Array) -> Array {
            Array::from([f64::NAN])
        }
    }
    let mut problem = Problem::new(&Nonfinite, &NoConstraint, Array::from([0.0]));
    let mut method = solver(Common::default());
    assert!(method.minimize(&mut problem, &criteria()).is_err());
    let result = method.last_result().unwrap();
    assert_eq!((result.status, result.nfev), (Termination::Nonfinite, 1));
    assert!(!result.success);
    assert_eq!(problem.function_evaluation(), 1);
}

#[test]
fn invalid_repeat_clears_result_without_resetting_problem_state() {
    let cost = SignedCost(Cell::new(0));
    let mut good = Problem::new(&cost, &NoConstraint, Array::from([0.0]));
    let mut method = solver(Common::default());
    method.minimize(&mut good, &criteria()).unwrap();
    let mut bad = Problem::new(&cost, &NoConstraint, Array::from([0.0, 0.0]));
    bad.set_function_value(13.0);
    bad.value(&Array::from([0.0]));
    assert!(method.minimize(&mut bad, &criteria()).is_err());
    assert!(method.last_result().is_none());
    assert_eq!(bad.function_value(), 13.0);
    assert_eq!(bad.function_evaluation(), 1);
}

#[test]
fn fixed_box_prices_once_and_records_convergence() {
    let cost = SignedCost(Cell::new(0));
    let mut problem = Problem::new(&cost, &NoConstraint, Array::from([0.0]));
    let mut method = Firefly::new(
        Bounds {
            lower: vec![0.25],
            upper: vec![0.25],
        },
        FireflyOptions::default(),
        Common::default(),
    )
    .unwrap();
    assert_eq!(
        method.minimize(&mut problem, &criteria()).unwrap(),
        EndCriteriaType::StationaryPoint
    );
    let result = method.last_result().unwrap();
    assert!(result.success);
    assert_eq!((result.nit, result.nfev), (0, 1));
    assert_eq!(result.x, vec![0.25]);
    assert_eq!(result.fun, -5.0);
    assert_eq!(cost.0.get(), 1);
}

#[path = "firefly_model_tests.rs"]
mod model;
#[path = "firefly_validation_tests.rs"]
mod validation;
