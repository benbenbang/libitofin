use super::*;
use crate::math::optimization::constraint::{BoundaryConstraint, Constraint, NoConstraint};
use crate::math::optimization::costfunction::CostFunction;
use std::cell::Cell;

struct SignedBowl;
impl CostFunction for SignedBowl {
    fn values(&self, _: &Array) -> Array {
        panic!("annealing must honor the signed scalar override")
    }
    fn value(&self, x: &Array) -> f64 {
        (x[0] - 0.25).powi(2) - 5.0
    }
    fn gradient(&self, _: &mut Array, _: &Array) {
        panic!("annealing must not evaluate gradients")
    }
}

fn solver(common: Common) -> HybridSimulatedAnnealing {
    HybridSimulatedAnnealing::new(
        Bounds {
            lower: vec![-1.0],
            upper: vec![1.0],
        },
        HybridSimulatedAnnealingOptions::default(),
        common,
    )
    .unwrap()
}

fn criteria() -> EndCriteria {
    EndCriteria::new(1000, Some(10), 1e-9, 1e-9, None).unwrap()
}

#[test]
fn signed_scalar_solution_and_cached_counts_match_the_problem() {
    let mut method = solver(Common::default());
    let mut problem = Problem::new(&SignedBowl, &NoConstraint, Array::from([0.0]));
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
    assert_eq!(result.fun, problem.function_value());
    assert_eq!(result.x, problem.current_value().to_vec());
    assert_eq!(result.nfev, problem.function_evaluation() as usize);
    assert_eq!(result.njev, 0);
    assert_eq!(problem.gradient_evaluation(), 0);
    assert_eq!(method.global_result(), method.last_result());
}

#[test]
fn seeded_repeat_is_identical() {
    let mut method = solver(Common::default());
    let mut first = Problem::new(&SignedBowl, &NoConstraint, Array::from([0.0]));
    method.minimize(&mut first, &criteria()).unwrap();
    let result = method.last_result().unwrap().clone();
    let mut second = Problem::new(&SignedBowl, &NoConstraint, Array::from([0.0]));
    method.minimize(&mut second, &criteria()).unwrap();
    assert_eq!(result, *method.last_result().unwrap());
}

#[test]
fn exhaustion_keeps_exact_counts_and_does_not_claim_convergence() {
    for (common, legacy, status, nit, nfev) in [
        (
            Common {
                maxfev: Some(1),
                ..Common::default()
            },
            EndCriteriaType::Unknown,
            Termination::MaxEvaluations,
            0,
            1,
        ),
        (
            Common {
                maxiter: Some(1),
                ..Common::default()
            },
            EndCriteriaType::MaxIterations,
            Termination::MaxIterations,
            1,
            2,
        ),
    ] {
        let mut method = solver(common);
        let mut problem = Problem::new(&SignedBowl, &NoConstraint, Array::from([0.0]));
        assert_eq!(method.minimize(&mut problem, &criteria()).unwrap(), legacy);
        let result = method.last_result().unwrap();
        assert_eq!(
            (result.status, result.nit, result.nfev),
            (status, nit, nfev)
        );
        assert!(!result.success);
        assert_eq!(result.nfev, problem.function_evaluation() as usize);
    }
}

#[test]
fn local_poll_exhaustion_is_charged_without_an_extra_pricing_call() {
    let mut method = HybridSimulatedAnnealing::new(
        Bounds {
            lower: vec![-1.0],
            upper: vec![1.0],
        },
        HybridSimulatedAnnealingOptions {
            local_search_interval: 1,
            ..HybridSimulatedAnnealingOptions::default()
        },
        Common {
            maxfev: Some(3),
            ..Common::default()
        },
    )
    .unwrap();
    let mut problem = Problem::new(&SignedBowl, &NoConstraint, Array::from([0.0]));
    assert_eq!(
        method.minimize(&mut problem, &criteria()).unwrap(),
        EndCriteriaType::Unknown
    );
    let result = method.last_result().unwrap();
    assert_eq!(result.status, Termination::MaxEvaluations);
    assert_eq!(result.nfev, 3);
    assert_eq!(problem.function_evaluation(), 3);
}

#[test]
fn end_criteria_caps_completed_iterations() {
    let mut method = solver(Common {
        maxiter: Some(100),
        ..Common::default()
    });
    let mut problem = Problem::new(&SignedBowl, &NoConstraint, Array::from([0.0]));
    let criteria = EndCriteria::new(3, Some(2), 0.0, 0.0, None).unwrap();
    assert_eq!(
        method.minimize(&mut problem, &criteria).unwrap(),
        EndCriteriaType::MaxIterations
    );
    let result = method.last_result().unwrap();
    assert_eq!((result.nit, result.nfev), (3, 4));
    assert_eq!(result.nfev, problem.function_evaluation() as usize);
}

#[test]
fn bounds_intersect_and_initial_candidate_is_clamped_before_pricing() {
    let constraint = BoundaryConstraint::new(-0.1, 0.1);
    let mut method = HybridSimulatedAnnealing::new(
        Bounds {
            lower: vec![0.05],
            upper: vec![1.0],
        },
        HybridSimulatedAnnealingOptions::default(),
        Common::default(),
    )
    .unwrap();
    let mut problem = Problem::new(&SignedBowl, &constraint, Array::from([0.0]));
    method.minimize(&mut problem, &criteria()).unwrap();
    let result = method.last_result().unwrap();
    assert!((0.05..=0.1).contains(&result.x[0]));
    assert!((result.x[0] - 0.1).abs() < 1e-4);
    assert_eq!(result.nfev, problem.function_evaluation() as usize);
}

#[test]
fn invalid_repeat_clears_result_without_resetting_existing_problem_state() {
    let mut method = solver(Common::default());
    let mut good = Problem::new(&SignedBowl, &NoConstraint, Array::from([0.0]));
    method.minimize(&mut good, &criteria()).unwrap();
    let mut bad = Problem::new(&SignedBowl, &NoConstraint, Array::from([0.0, 0.0]));
    bad.set_function_value(13.0);
    bad.value(&Array::from([0.0]));
    assert!(method.minimize(&mut bad, &criteria()).is_err());
    assert!(method.last_result().is_none());
    assert_eq!(bad.function_value(), 13.0);
    assert_eq!(bad.function_evaluation(), 1);
}

#[test]
fn coupled_constraint_rejects_candidate_before_pricing() {
    struct OnlyOrigin;
    impl Constraint for OnlyOrigin {
        fn test(&self, x: &Array) -> bool {
            x[0] == 0.0
        }
    }
    struct Counting(Cell<usize>);
    impl CostFunction for Counting {
        fn values(&self, x: &Array) -> Array {
            self.0.set(self.0.get() + 1);
            assert_eq!(x[0], 0.0);
            Array::from([1.0])
        }
    }
    let cost = Counting(Cell::new(0));
    let mut problem = Problem::new(&cost, &OnlyOrigin, Array::from([0.0]));
    let mut method = solver(Common::default());
    let error = method.minimize(&mut problem, &criteria()).unwrap_err();
    assert!(
        error
            .message()
            .contains("hybrid simulated annealing candidate")
    );
    assert!(error.message().contains("wholly feasible box"));
    assert!(method.last_result().is_none());
    assert_eq!(cost.0.get(), 1);
    assert_eq!(problem.function_evaluation(), 1);
    assert_eq!(problem.current_value(), &Array::from([0.0]));
}

#[test]
fn nonfinite_pricing_preserves_diagnostic_and_returns_error() {
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
    assert_eq!(result.status, Termination::Nonfinite);
    assert!(!result.success);
    assert_eq!(result.nfev, 1);
    assert_eq!(problem.function_evaluation(), 1);
}

#[test]
fn constructor_rejects_invalid_bounds_schedules_and_unsafe_legacy_counts() {
    for upper in [f64::INFINITY, f64::NAN, -1.0] {
        assert!(
            HybridSimulatedAnnealing::new(
                Bounds {
                    lower: vec![0.0],
                    upper: vec![upper]
                },
                HybridSimulatedAnnealingOptions::default(),
                Common::default(),
            )
            .is_err()
        );
    }
    for options in [
        HybridSimulatedAnnealingOptions {
            initial_temperature: 0.0,
            ..HybridSimulatedAnnealingOptions::default()
        },
        HybridSimulatedAnnealingOptions {
            cooling_rate: 1.0,
            ..HybridSimulatedAnnealingOptions::default()
        },
        HybridSimulatedAnnealingOptions {
            local_search_interval: 0,
            ..HybridSimulatedAnnealingOptions::default()
        },
    ] {
        assert!(
            HybridSimulatedAnnealing::new(
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
    assert!(solver_result_with_budget(10_000_001).is_err());
}

fn solver_result_with_budget(maxfev: usize) -> QlResult<HybridSimulatedAnnealing> {
    HybridSimulatedAnnealing::new(
        Bounds {
            lower: vec![0.0],
            upper: vec![1.0],
        },
        HybridSimulatedAnnealingOptions::default(),
        Common {
            maxfev: Some(maxfev),
            ..Common::default()
        },
    )
}

#[test]
fn empty_intersection_fails_before_reset_or_pricing() {
    let constraint = BoundaryConstraint::new(2.0, 3.0);
    let mut problem = Problem::new(&SignedBowl, &constraint, Array::from([2.5]));
    problem.set_function_value(17.0);
    let mut method = solver(Common::default());
    let error = method.minimize(&mut problem, &criteria()).unwrap_err();
    assert!(error.message().contains("empty constraint intersection"));
    assert_eq!(problem.function_value(), 17.0);
    assert_eq!(problem.function_evaluation(), 0);
    assert!(method.last_result().is_none());
}

#[test]
fn fixed_box_evaluates_scalar_once_and_maps_success() {
    let mut method = HybridSimulatedAnnealing::new(
        Bounds {
            lower: vec![0.25],
            upper: vec![0.25],
        },
        HybridSimulatedAnnealingOptions::default(),
        Common::default(),
    )
    .unwrap();
    let mut problem = Problem::new(&SignedBowl, &NoConstraint, Array::from([0.0]));
    assert_eq!(
        method.minimize(&mut problem, &criteria()).unwrap(),
        EndCriteriaType::StationaryPoint
    );
    let result = method.last_result().unwrap();
    assert!(result.success);
    assert_eq!((result.nit, result.nfev), (0, 1));
    assert_eq!(result.x, vec![0.25]);
    assert_eq!(result.fun, -5.0);
}

#[path = "hybrid_simulated_annealing_model_tests.rs"]
mod model;
