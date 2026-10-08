use super::*;
use crate::models::calibrationhelper::CalibrationHelper;
use crate::models::model::{CalibratedModel, CalibratedModelHolder, calibrate};
use crate::models::parameter::ConstantParameter;
use crate::shared::{SharedMut, shared_mut};
use std::rc::Rc;

struct LinearModel(CalibratedModel);
impl CalibratedModelHolder for LinearModel {
    fn calibrated_model(&self) -> &CalibratedModel {
        &self.0
    }
    fn calibrated_model_mut(&mut self) -> &mut CalibratedModel {
        &mut self.0
    }
}
struct LinearHelper {
    model: SharedMut<LinearModel>,
    index: usize,
    target: f64,
    calls: Rc<Cell<usize>>,
    fail: bool,
}
impl CalibrationHelper for LinearHelper {
    fn calibration_error(&mut self) -> QlResult<f64> {
        self.calls.set(self.calls.get() + 1);
        let model = self.model.borrow();
        let params = model.0.params();
        assert_eq!(params[1], 3.0);
        if self.fail {
            return Err(ql_error("test pricing failure"));
        }
        Ok(params[self.index] - self.target)
    }
}
fn linear_model() -> SharedMut<LinearModel> {
    let mut model = CalibratedModel::new(3);
    for (index, value) in [0.5, 3.0, 12.0].into_iter().enumerate() {
        model.arguments_mut()[index] =
            ConstantParameter::new(value, Rc::new(NoConstraint)).unwrap();
    }
    shared_mut(LinearModel(model))
}
fn linear_helpers(
    model: &SharedMut<LinearModel>,
    fail: bool,
) -> (Vec<SharedMut<dyn CalibrationHelper>>, Rc<Cell<usize>>) {
    let calls = Rc::new(Cell::new(0));
    let helpers = [(0, 0.25), (2, 14.0)]
        .into_iter()
        .map(|(index, target)| {
            shared_mut(LinearHelper {
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
fn linear_method(common: Common) -> Firefly {
    Firefly::new(
        Bounds {
            lower: vec![-1.0, 10.0],
            upper: vec![1.0, 20.0],
        },
        FireflyOptions::default(),
        common,
    )
    .unwrap()
}

#[test]
fn projected_nonadjacent_parameters_preserve_weighted_rss_and_true_counts() {
    let model = linear_model();
    let (helpers, calls) = linear_helpers(&model, false);
    let mut method = linear_method(Common::default());
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
    let expected = (4.0 * (params[0] - 0.25).powi(2) + 9.0 * (params[2] - 14.0).powi(2)).sqrt();
    assert!((result.fun - expected).abs() < 1e-12);
}

#[test]
fn evaluation_budget_returns_root_sum_of_squares_not_residual_mean() {
    let model = linear_model();
    let (helpers, calls) = linear_helpers(&model, false);
    let mut method = linear_method(Common {
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
    assert_eq!(
        (result.status, result.nit, result.nfev),
        (Termination::MaxEvaluations, 0, 1)
    );
    assert!((result.fun - 36.25_f64.sqrt()).abs() < 1e-12);
    assert_eq!(model.borrow().0.end_criteria(), EndCriteriaType::Unknown);
    assert_eq!(model.borrow().0.function_evaluation(), 2);
    assert_eq!(calls.get(), 4);
}

#[test]
fn coupled_candidate_constraint_aborts_before_pricing_and_restores_model() {
    struct OnlyInitial;
    impl Constraint for OnlyInitial {
        fn test(&self, x: &Array) -> bool {
            x[0] == 0.5 && x[2] == 12.0
        }
    }
    let model = linear_model();
    let original = model.borrow().0.params();
    let (helpers, calls) = linear_helpers(&model, false);
    let mut method = linear_method(Common::default());
    let error = calibrate(
        &model,
        &helpers,
        &mut method,
        &criteria(),
        Some(Box::new(OnlyInitial)),
        vec![],
        vec![false, true, false],
    )
    .unwrap_err();
    assert!(error.message().contains("wholly feasible box"));
    assert_eq!(calls.get(), 2);
    assert_eq!(model.borrow().0.params(), original);
    assert!(method.last_result().is_none());
    assert_eq!(model.borrow().0.end_criteria(), EndCriteriaType::None);
    assert_eq!(model.borrow().0.function_evaluation(), 0);
}

#[test]
fn pricing_failure_restores_model_and_preserves_nonfinite_diagnostic() {
    let model = linear_model();
    let original = model.borrow().0.params();
    let (helpers, calls) = linear_helpers(&model, true);
    let mut method = linear_method(Common::default());
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
    let result = method.last_result().unwrap();
    assert_eq!((result.status, result.nfev), (Termination::Nonfinite, 1));
}

#[test]
fn model_preflight_rejection_retains_previous_diagnostics_without_pricing() {
    let model = linear_model();
    let (helpers, calls) = linear_helpers(&model, false);
    let mut method = linear_method(Common {
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
    let params = model.borrow().0.params();
    let count = calls.get();
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
        assert_eq!(model.borrow().0.params(), params);
        assert_eq!(calls.get(), count);
    }
}
