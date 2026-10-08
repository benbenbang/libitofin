use super::*;
use crate::math::optimization::constraint::{BoundaryConstraint, Constraint, NoConstraint};
use crate::math::optimization::costfunction::CostFunction;
use itofin_optimize::GlobalOptions;
use std::cell::Cell;

struct NegativeSquare;
impl CostFunction for NegativeSquare {
    fn values(&self, _: &Array) -> Array {
        panic!("particle swarm must honor the scalar value override")
    }
    fn value(&self, x: &Array) -> f64 {
        (x[0] - 0.25).powi(2) - 5.0
    }
    fn gradient(&self, _: &mut Array, _: &Array) {
        panic!("particle swarm must not evaluate gradients")
    }
}

fn solver(common: Common) -> ParticleSwarm {
    ParticleSwarm::new(
        Bounds {
            lower: vec![-1.0],
            upper: vec![1.0],
        },
        ParticleSwarmOptions::default(),
        common,
    )
    .unwrap()
}

fn criteria() -> EndCriteria {
    EndCriteria::new(500, Some(10), 1e-9, 1e-9, None).unwrap()
}

#[test]
fn scalar_override_negative_values_and_exact_cached_counts() {
    let mut method = solver(Common::default());
    let mut problem = Problem::new(&NegativeSquare, &NoConstraint, Array::from([0.0]));
    assert!(
        method
            .minimize(&mut problem, &criteria())
            .unwrap()
            .succeeded()
    );
    let result = method.last_result().unwrap();
    assert!(result.success);
    assert!((result.fun + 5.0).abs() < 1e-6);
    assert_eq!(result.fun, problem.function_value());
    assert_eq!(result.x, problem.current_value().to_vec());
    assert_eq!(result.nfev, problem.function_evaluation() as usize);
    assert_eq!(result.njev, 0);
    assert_eq!(problem.gradient_evaluation(), 0);
    assert_eq!(method.global_result(), method.last_result());
}

#[test]
fn seeded_repeat_has_identical_outcome() {
    let mut method = solver(Common::default());
    let mut first = Problem::new(&NegativeSquare, &NoConstraint, Array::from([0.0]));
    method.minimize(&mut first, &criteria()).unwrap();
    let result = method.last_result().unwrap().clone();
    let mut second = Problem::new(&NegativeSquare, &NoConstraint, Array::from([0.0]));
    method.minimize(&mut second, &criteria()).unwrap();
    assert_eq!(result, *method.last_result().unwrap());
}

#[test]
fn exhaustion_maps_exact_counts_without_claiming_convergence() {
    for (common, expected, status, nit, nfev) in [
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
            30,
        ),
    ] {
        let mut method = solver(common);
        let mut problem = Problem::new(&NegativeSquare, &NoConstraint, Array::from([0.0]));
        assert_eq!(
            method.minimize(&mut problem, &criteria()).unwrap(),
            expected
        );
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
fn end_criteria_caps_completed_generations() {
    let mut method = solver(Common {
        maxiter: Some(100),
        ..Common::default()
    });
    let mut problem = Problem::new(&NegativeSquare, &NoConstraint, Array::from([0.0]));
    let criteria = EndCriteria::new(3, Some(2), 0.0, 0.0, None).unwrap();
    assert_eq!(
        method.minimize(&mut problem, &criteria).unwrap(),
        EndCriteriaType::MaxIterations
    );
    let result = method.last_result().unwrap();
    assert_eq!((result.nit, result.nfev), (3, 60));
    assert_eq!(result.nfev, problem.function_evaluation() as usize);
}

#[test]
fn intersects_bounds_and_clamps_initial_before_pricing() {
    let constraint = BoundaryConstraint::new(-0.1, 0.1);
    let mut method = ParticleSwarm::new(
        Bounds {
            lower: vec![0.05],
            upper: vec![1.0],
        },
        ParticleSwarmOptions::default(),
        Common::default(),
    )
    .unwrap();
    let mut problem = Problem::new(&NegativeSquare, &constraint, Array::from([0.0]));
    method.minimize(&mut problem, &criteria()).unwrap();
    let result = method.last_result().unwrap();
    assert!((0.05..=0.1).contains(&result.x[0]));
    assert!((result.x[0] - 0.1).abs() < 1e-4);
    assert_eq!(result.nfev, problem.function_evaluation() as usize);
}

#[test]
fn invalid_repeat_clears_result_without_resetting_problem() {
    let mut method = solver(Common::default());
    let mut good = Problem::new(&NegativeSquare, &NoConstraint, Array::from([0.0]));
    method.minimize(&mut good, &criteria()).unwrap();
    let mut bad = Problem::new(&NegativeSquare, &NoConstraint, Array::from([0.0, 0.0]));
    bad.set_function_value(13.0);
    bad.value(&Array::from([0.0]));
    assert!(method.minimize(&mut bad, &criteria()).is_err());
    assert!(method.last_result().is_none());
    assert_eq!(bad.function_value(), 13.0);
    assert_eq!(bad.function_evaluation(), 1);
}

#[test]
fn rejected_candidate_is_not_priced_or_committed() {
    struct RejectNonzero;
    impl Constraint for RejectNonzero {
        fn test(&self, x: &Array) -> bool {
            x[0] == 0.0
        }
    }
    struct CountCost(Cell<usize>);
    impl CostFunction for CountCost {
        fn values(&self, x: &Array) -> Array {
            self.0.set(self.0.get() + 1);
            assert_eq!(x[0], 0.0);
            Array::from([1.0])
        }
    }
    let cost = CountCost(Cell::new(0));
    let mut problem = Problem::new(&cost, &RejectNonzero, Array::from([0.0]));
    let mut method = solver(Common::default());
    let error = method.minimize(&mut problem, &criteria()).unwrap_err();
    assert!(error.message().contains("particle swarm candidate"));
    assert!(error.message().contains("wholly feasible box"));
    assert!(method.last_result().is_none());
    assert_eq!(problem.function_evaluation() as usize, cost.0.get());
    assert_eq!(cost.0.get(), 1);
    assert_eq!(problem.current_value(), &Array::from([0.0]));
}

#[test]
fn nonfinite_termination_retains_diagnostic_but_errors() {
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
    assert_eq!(result.nfev, problem.function_evaluation() as usize);
}

#[test]
fn constructor_rejects_invalid_bounds_coefficients_and_budgets() {
    for upper in [f64::INFINITY, f64::NAN, -1.0] {
        assert!(
            ParticleSwarm::new(
                Bounds {
                    lower: vec![0.0],
                    upper: vec![upper]
                },
                ParticleSwarmOptions::default(),
                Common::default(),
            )
            .is_err()
        );
    }
    assert!(
        ParticleSwarm::new(
            Bounds {
                lower: vec![0.0],
                upper: vec![1.0]
            },
            ParticleSwarmOptions {
                inertia: -1.0,
                ..ParticleSwarmOptions::default()
            },
            Common::default(),
        )
        .is_err()
    );
    assert!(
        ParticleSwarm::new(
            Bounds {
                lower: vec![0.0],
                upper: vec![1.0]
            },
            ParticleSwarmOptions::default(),
            Common {
                maxfev: Some(10_000_001),
                ..Common::default()
            },
        )
        .is_err()
    );
}

#[test]
fn explicit_population_outside_intersection_rejected_before_reset() {
    let mut method = ParticleSwarm::new(
        Bounds {
            lower: vec![-1.0],
            upper: vec![1.0],
        },
        ParticleSwarmOptions {
            global: GlobalOptions {
                initial_population: Some(vec![vec![0.0], vec![0.1], vec![0.2], vec![0.3]]),
                ..GlobalOptions::default()
            },
            ..ParticleSwarmOptions::default()
        },
        Common::default(),
    )
    .unwrap();
    let constraint = BoundaryConstraint::new(-0.1, 0.1);
    let mut problem = Problem::new(&NegativeSquare, &constraint, Array::from([0.0]));
    problem.set_function_value(77.0);
    assert!(method.minimize(&mut problem, &criteria()).is_err());
    assert_eq!(problem.function_value(), 77.0);
    assert_eq!(problem.function_evaluation(), 0);
}

#[test]
fn empty_intersection_rejected_without_reset_or_evaluation() {
    let constraint = BoundaryConstraint::new(2.0, 3.0);
    let mut problem = Problem::new(&NegativeSquare, &constraint, Array::from([2.5]));
    problem.set_function_value(17.0);
    let mut method = solver(Common::default());
    let error = method.minimize(&mut problem, &criteria()).unwrap_err();
    assert!(error.message().contains("empty constraint intersection"));
    assert_eq!(problem.function_value(), 17.0);
    assert_eq!(problem.function_evaluation(), 0);
    assert!(method.last_result().is_none());
}

#[test]
fn fixed_box_calls_scalar_once_and_maps_success() {
    let mut method = ParticleSwarm::new(
        Bounds {
            lower: vec![0.25],
            upper: vec![0.25],
        },
        ParticleSwarmOptions::default(),
        Common::default(),
    )
    .unwrap();
    let mut problem = Problem::new(&NegativeSquare, &NoConstraint, Array::from([0.0]));
    assert_eq!(
        method.minimize(&mut problem, &criteria()).unwrap(),
        EndCriteriaType::StationaryPoint
    );
    let result = method.last_result().unwrap();
    assert!(result.success);
    assert_eq!((result.nit, result.nfev), (0, 1));
    assert_eq!(result.x, vec![0.25]);
    assert_eq!(result.fun, -5.0);
    assert_eq!(problem.function_evaluation(), 1);
}

#[path = "particle_swarm_model_tests.rs"]
mod model;
