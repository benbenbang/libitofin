use super::*;
use crate::math::optimization::constraint::{BoundaryConstraint, Constraint, NoConstraint};
use crate::math::optimization::costfunction::CostFunction;
use itofin_optimize::GlobalOptions;
use std::cell::Cell;

struct NegativeSquare;
impl CostFunction for NegativeSquare {
    fn values(&self, x: &Array) -> Array {
        Array::from([100.0 + x[0]])
    }
    fn value(&self, x: &Array) -> f64 {
        (x[0] - 0.25).powi(2) - 5.0
    }
}

fn solver(common: Common) -> DifferentialEvolution {
    DifferentialEvolution::new(
        Bounds {
            lower: vec![-1.0],
            upper: vec![1.0],
        },
        DifferentialEvolutionOptions::default(),
        common,
    )
    .unwrap()
}

fn criteria() -> EndCriteria {
    EndCriteria::new(500, Some(10), 1e-9, 1e-9, None).unwrap()
}

#[test]
fn scalar_override_negative_values_and_exact_cached_counts() {
    let mut solver = solver(Common::default());
    let mut problem = Problem::new(&NegativeSquare, &NoConstraint, Array::from([0.0]));
    let status = solver.minimize(&mut problem, &criteria()).unwrap();
    assert!(status.succeeded());
    let result = solver.last_result().unwrap();
    assert!(result.success);
    assert!((result.fun + 5.0).abs() < 1e-6);
    assert_eq!(result.fun, problem.function_value());
    assert_eq!(result.x, problem.current_value().to_vec());
    assert_eq!(result.nfev, problem.function_evaluation() as usize);
    assert_eq!(result.njev, 0);
    assert_eq!(problem.gradient_evaluation(), 0);
}

#[test]
fn seeded_repeat_has_identical_outcome() {
    let mut solver = solver(Common::default());
    let mut first = Problem::new(&NegativeSquare, &NoConstraint, Array::from([0.0]));
    solver.minimize(&mut first, &criteria()).unwrap();
    let result = solver.last_result().unwrap().clone();
    let mut second = Problem::new(&NegativeSquare, &NoConstraint, Array::from([0.0]));
    solver.minimize(&mut second, &criteria()).unwrap();
    assert_eq!(result, *solver.last_result().unwrap());
}

#[test]
fn exhaustion_is_not_legacy_convergence() {
    for (common, expected, status) in [
        (
            Common {
                maxfev: Some(1),
                ..Common::default()
            },
            EndCriteriaType::Unknown,
            Termination::MaxEvaluations,
        ),
        (
            Common {
                maxiter: Some(1),
                ..Common::default()
            },
            EndCriteriaType::MaxIterations,
            Termination::MaxIterations,
        ),
    ] {
        let mut solver = solver(common);
        let mut problem = Problem::new(&NegativeSquare, &NoConstraint, Array::from([0.0]));
        assert_eq!(
            solver.minimize(&mut problem, &criteria()).unwrap(),
            expected
        );
        let result = solver.last_result().unwrap();
        assert_eq!(result.status, status);
        assert!(!result.success);
        assert_eq!(result.nfev, problem.function_evaluation() as usize);
    }
}

#[test]
fn intersects_constraint_bounds_without_extra_evaluation() {
    let constraint = BoundaryConstraint::new(-0.1, 0.1);
    let mut solver = solver(Common::default());
    let mut problem = Problem::new(&NegativeSquare, &constraint, Array::from([0.0]));
    solver.minimize(&mut problem, &criteria()).unwrap();
    let result = solver.last_result().unwrap();
    assert!(result.x[0] <= 0.1 && result.x[0] >= -0.1);
    assert!((result.x[0] - 0.1).abs() < 1e-4);
    assert_eq!(result.nfev, problem.function_evaluation() as usize);
}

#[test]
fn invalid_repeat_clears_result_and_preserves_problem_state() {
    let mut solver = solver(Common::default());
    let mut good = Problem::new(&NegativeSquare, &NoConstraint, Array::from([0.0]));
    solver.minimize(&mut good, &criteria()).unwrap();
    let mut bad = Problem::new(&NegativeSquare, &NoConstraint, Array::from([0.0, 0.0]));
    bad.set_function_value(13.0);
    bad.value(&Array::from([0.0]));
    assert!(solver.minimize(&mut bad, &criteria()).is_err());
    assert!(solver.last_result().is_none());
    assert_eq!(bad.function_value(), 13.0);
    assert_eq!(bad.function_evaluation(), 1);
}

#[test]
fn invalid_candidates_are_never_priced() {
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
    let mut solver = solver(Common::default());
    let error = solver.minimize(&mut problem, &criteria()).unwrap_err();
    assert!(error.message().contains("wholly feasible box"));
    assert!(solver.last_result().is_none());
    assert_eq!(problem.function_evaluation() as usize, cost.0.get());
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
    let mut solver = solver(Common::default());
    assert!(solver.minimize(&mut problem, &criteria()).is_err());
    let result = solver.last_result().unwrap();
    assert_eq!(result.status, Termination::Nonfinite);
    assert!(!result.success);
    assert_eq!(result.nfev, problem.function_evaluation() as usize);
}

#[test]
fn constructor_rejects_unbounded_and_overflow_counts() {
    for upper in [f64::INFINITY, f64::NAN] {
        assert!(
            DifferentialEvolution::new(
                Bounds {
                    lower: vec![0.0],
                    upper: vec![upper]
                },
                DifferentialEvolutionOptions::default(),
                Common::default(),
            )
            .is_err()
        );
    }
    assert!(
        DifferentialEvolution::new(
            Bounds {
                lower: vec![0.0],
                upper: vec![1.0]
            },
            DifferentialEvolutionOptions::default(),
            Common {
                maxfev: Some(10_000_001),
                ..Common::default()
            },
        )
        .is_err()
    );
}

#[test]
fn explicit_population_rejected_before_problem_reset() {
    let options = DifferentialEvolutionOptions {
        global: GlobalOptions {
            initial_population: Some(vec![vec![0.0], vec![0.1], vec![0.2], vec![0.3]]),
            ..GlobalOptions::default()
        },
        ..DifferentialEvolutionOptions::default()
    };
    let mut solver = DifferentialEvolution::new(
        Bounds {
            lower: vec![-1.0],
            upper: vec![1.0],
        },
        options,
        Common::default(),
    )
    .unwrap();
    let constraint = BoundaryConstraint::new(-0.1, 0.1);
    let mut problem = Problem::new(&NegativeSquare, &constraint, Array::from([0.0]));
    problem.set_function_value(77.0);
    assert!(solver.minimize(&mut problem, &criteria()).is_err());
    assert_eq!(problem.function_value(), 77.0);
    assert_eq!(problem.function_evaluation(), 0);
}

use crate::models::calibrationhelper::CalibrationHelper;
use crate::models::model::{CalibratedModel, CalibratedModelHolder, calibrate};
use crate::models::parameter::ConstantParameter;
use crate::shared::{SharedMut, shared_mut};
use std::rc::Rc;

struct FitModel(CalibratedModel);
impl CalibratedModelHolder for FitModel {
    fn calibrated_model(&self) -> &CalibratedModel {
        &self.0
    }
    fn calibrated_model_mut(&mut self) -> &mut CalibratedModel {
        &mut self.0
    }
}
struct FitHelper(SharedMut<FitModel>);
impl CalibrationHelper for FitHelper {
    fn calibration_error(&mut self) -> QlResult<f64> {
        let params = self.0.borrow().0.params();
        assert_eq!(params[0], 3.0);
        Ok(params[1] - 0.25)
    }
}
fn model_and_helper() -> (SharedMut<FitModel>, Vec<SharedMut<dyn CalibrationHelper>>) {
    let mut model = CalibratedModel::new(2);
    model.arguments_mut()[0] = ConstantParameter::new(3.0, Rc::new(NoConstraint)).unwrap();
    model.arguments_mut()[1] = ConstantParameter::new(0.5, Rc::new(NoConstraint)).unwrap();
    let model = shared_mut(FitModel(model));
    let helper = shared_mut(FitHelper(model.clone())) as SharedMut<dyn CalibrationHelper>;
    (model, vec![helper])
}

#[test]
fn model_calibration_preserves_fixed_parameters_and_accounts_final_residual() {
    let (model, helpers) = model_and_helper();
    let mut method = solver(Common::default());
    calibrate(
        &model,
        &helpers,
        &mut method,
        &criteria(),
        None,
        vec![],
        vec![true, false],
    )
    .unwrap();
    let result = method.last_result().unwrap();
    let model = model.borrow();
    assert_eq!(model.0.params()[0], 3.0);
    assert!((model.0.params()[1] - 0.25).abs() < 1e-4);
    assert_eq!(model.0.function_evaluation() as usize, result.nfev + 1);
    assert_eq!(result.x.len(), 1);
}

#[test]
fn model_calibration_rolls_back_on_unfeasible_candidate() {
    struct BelowPointSeven;
    impl Constraint for BelowPointSeven {
        fn test(&self, x: &Array) -> bool {
            x[1] < 0.7
        }
    }
    let (model, helpers) = model_and_helper();
    let original = model.borrow().0.params();
    let options = DifferentialEvolutionOptions {
        global: GlobalOptions {
            initial_population: Some(vec![vec![0.5], vec![0.6], vec![0.8], vec![0.9]]),
            ..GlobalOptions::default()
        },
        ..DifferentialEvolutionOptions::default()
    };
    let mut method = DifferentialEvolution::new(
        Bounds {
            lower: vec![-1.0],
            upper: vec![1.0],
        },
        options,
        Common::default(),
    )
    .unwrap();
    assert!(
        calibrate(
            &model,
            &helpers,
            &mut method,
            &criteria(),
            Some(Box::new(BelowPointSeven)),
            vec![],
            vec![true, false]
        )
        .is_err()
    );
    assert_eq!(model.borrow().0.params(), original);
    assert!(method.last_result().is_none());
}
