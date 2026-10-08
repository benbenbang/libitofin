use super::*;
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
struct FitHelper {
    model: SharedMut<FitModel>,
    index: usize,
    target: f64,
    calls: Rc<Cell<usize>>,
    fail: bool,
}
impl CalibrationHelper for FitHelper {
    fn calibration_error(&mut self) -> QlResult<f64> {
        self.calls.set(self.calls.get() + 1);
        let params = self.model.borrow().0.params();
        assert_eq!(params[1], 3.0);
        if self.fail {
            return Err(ql_error("pricing failed"));
        }
        Ok(params[self.index] - self.target)
    }
}
fn model() -> SharedMut<FitModel> {
    let mut model = CalibratedModel::new(3);
    for (index, value) in [0.5, 3.0, 12.0].into_iter().enumerate() {
        model.arguments_mut()[index] =
            ConstantParameter::new(value, Rc::new(NoConstraint)).unwrap();
    }
    shared_mut(FitModel(model))
}
fn helpers(
    model: &SharedMut<FitModel>,
    fail: bool,
) -> (Vec<SharedMut<dyn CalibrationHelper>>, Rc<Cell<usize>>) {
    let calls = Rc::new(Cell::new(0));
    let helpers = [(0, 0.25), (2, 14.0)]
        .into_iter()
        .map(|(index, target)| {
            shared_mut(FitHelper {
                model: model.clone(),
                index,
                target,
                calls: calls.clone(),
                fail,
            }) as SharedMut<dyn CalibrationHelper>
        })
        .collect();
    (helpers, calls)
}
fn method(common: Common) -> ParticleSwarm {
    ParticleSwarm::new(
        Bounds {
            lower: vec![-1.0, 10.0],
            upper: vec![1.0, 20.0],
        },
        ParticleSwarmOptions::default(),
        common,
    )
    .unwrap()
}

#[test]
fn model_projection_keeps_nonadjacent_free_parameter_order_and_exact_counts() {
    let model = model();
    let (helpers, calls) = helpers(&model, false);
    let mut method = method(Common::default());
    calibrate(
        &model,
        &helpers,
        &mut method,
        &criteria(),
        None,
        vec![4.0, 9.0],
        vec![false, true, false],
    )
    .unwrap();
    let result = method.last_result().unwrap();
    let model = model.borrow();
    let params = model.0.params();
    assert_eq!(params[1], 3.0);
    assert!((params[0] - 0.25).abs() < 1e-4);
    assert!((params[2] - 14.0).abs() < 1e-4);
    assert_eq!(result.x, vec![params[0], params[2]]);
    assert_eq!(model.0.function_evaluation() as usize, result.nfev + 1);
    assert_eq!(calls.get(), 2 * (result.nfev + 1));
    assert!(
        (result.fun
            - model
                .0
                .problem_values()
                .iter()
                .map(|v| v * v)
                .sum::<f64>()
                .sqrt())
        .abs()
            < 1e-12
    );
}

#[test]
fn model_cost_remains_weighted_root_sum_of_squares_not_mean() {
    let model = model();
    let (helpers, calls) = helpers(&model, false);
    let mut method = method(Common {
        maxfev: Some(1),
        ..Common::default()
    });
    calibrate(
        &model,
        &helpers,
        &mut method,
        &criteria(),
        None,
        vec![4.0, 9.0],
        vec![false, true, false],
    )
    .unwrap();
    let result = method.last_result().unwrap();
    assert_eq!((result.nit, result.nfev), (0, 1));
    assert_eq!(result.status, Termination::MaxEvaluations);
    assert!((result.fun - (0.25_f64 * 0.25 * 4.0 + 4.0 * 9.0).sqrt()).abs() < 1e-12);
    assert_eq!(model.borrow().0.end_criteria(), EndCriteriaType::Unknown);
    assert_eq!(model.borrow().0.function_evaluation(), 2);
    assert_eq!(calls.get(), 4);
}

#[test]
fn model_candidate_constraint_precedes_pricing_and_failure_rolls_back() {
    struct BelowPointSeven;
    impl Constraint for BelowPointSeven {
        fn test(&self, x: &Array) -> bool {
            x[0] < 0.7
        }
    }
    let model = model();
    let original = model.borrow().0.params();
    let (helpers, calls) = helpers(&model, false);
    let mut method = ParticleSwarm::new(
        Bounds {
            lower: vec![-1.0, 10.0],
            upper: vec![1.0, 20.0],
        },
        ParticleSwarmOptions {
            global: GlobalOptions {
                initial_population: Some(vec![
                    vec![0.5, 12.0],
                    vec![0.6, 13.0],
                    vec![0.8, 14.0],
                    vec![0.9, 15.0],
                ]),
                ..GlobalOptions::default()
            },
            ..ParticleSwarmOptions::default()
        },
        Common::default(),
    )
    .unwrap();
    let error = calibrate(
        &model,
        &helpers,
        &mut method,
        &criteria(),
        Some(Box::new(BelowPointSeven)),
        vec![],
        vec![false, true, false],
    )
    .unwrap_err();
    assert!(error.message().contains("wholly feasible box"));
    assert_eq!(calls.get(), 4);
    assert_eq!(model.borrow().0.params(), original);
    assert!(method.last_result().is_none());
    assert_eq!(model.borrow().0.end_criteria(), EndCriteriaType::None);
    assert_eq!(model.borrow().0.function_evaluation(), 0);
}

#[test]
fn pricing_failure_rolls_back_and_retains_nonfinite_diagnostic() {
    let model = model();
    let original = model.borrow().0.params();
    let (helpers, calls) = helpers(&model, true);
    let mut method = method(Common::default());
    assert!(
        calibrate(
            &model,
            &helpers,
            &mut method,
            &criteria(),
            None,
            vec![],
            vec![false, true, false]
        )
        .is_err()
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(model.borrow().0.params(), original);
    assert_eq!(method.last_result().unwrap().status, Termination::Nonfinite);
    assert_eq!(method.last_result().unwrap().nfev, 1);
}

#[test]
fn model_preflight_failures_retain_previous_result_without_mutation_or_pricing() {
    let model = model();
    let (helpers, calls) = helpers(&model, false);
    let mut method = method(Common {
        maxfev: Some(1),
        ..Common::default()
    });
    calibrate(
        &model,
        &helpers,
        &mut method,
        &criteria(),
        None,
        vec![],
        vec![false, true, false],
    )
    .unwrap();
    let result = method.last_result().unwrap().clone();
    let original = model.borrow().0.params();
    let original_calls = calls.get();
    for (weights, fixed) in [
        (vec![-1.0, 1.0], vec![false, true, false]),
        (vec![], vec![true, true, true]),
        (vec![], vec![false]),
    ] {
        assert!(
            calibrate(
                &model,
                &helpers,
                &mut method,
                &criteria(),
                None,
                weights,
                fixed
            )
            .is_err()
        );
        assert_eq!(method.last_result(), Some(&result));
        assert_eq!(model.borrow().0.params(), original);
        assert_eq!(calls.get(), original_calls);
    }
    assert!(calibrate(&model, &[], &mut method, &criteria(), None, vec![], vec![]).is_err());
    assert_eq!(method.last_result(), Some(&result));
}
