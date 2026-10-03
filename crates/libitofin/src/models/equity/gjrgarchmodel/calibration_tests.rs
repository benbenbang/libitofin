use super::tests::{array, model, parameters};
use super::*;
use crate::math::optimization::endcriteria::{EndCriteria, EndCriteriaType};
use crate::math::optimization::method::OptimizationMethod;
use crate::math::optimization::problem::Problem;
use crate::math::optimization::simplex::Simplex;
use crate::models::calibrate;
use crate::models::calibrationhelper::CalibrationHelper;

struct VarianceHelper {
    model: SharedMut<GjrGarchModel>,
    annual_target: Real,
}

impl CalibrationHelper for VarianceHelper {
    fn calibration_error(&mut self) -> QlResult<Real> {
        Ok(self.model.borrow().process().initial_values()?[1] - self.annual_target)
    }
}

fn criteria() -> EndCriteria {
    EndCriteria::new(1000, Some(50), 1e-12, 1e-12, None).unwrap()
}

fn helper(model: &SharedMut<GjrGarchModel>) -> SharedMut<dyn CalibrationHelper> {
    shared_mut(VarianceHelper {
        model: SharedMut::clone(model),
        annual_target: 0.12,
    })
}

#[test]
fn calibration_rebuilds_process_and_preserves_fixed_parameters() {
    let model = model();
    let seed = model.borrow().calibrated_model().params();
    let mut method = Simplex::new(0.0001);
    calibrate(
        &model,
        &[helper(&model)],
        &mut method,
        &criteria(),
        None,
        Vec::new(),
        vec![true, true, true, true, true, false],
    )
    .unwrap();
    let borrowed = model.borrow();
    let fitted = borrowed.calibrated_model().params();
    for index in 0..5 {
        assert_eq!(fitted[index], seed[index]);
    }
    assert!((fitted[5] - 0.12 / 365.0).abs() < 1e-10);
    assert_eq!(array(borrowed.process().parameters()), fitted);
    assert_eq!(borrowed.process().days_per_year(), 365.0);
    assert!(borrowed.calibrated_model().end_criteria().succeeded());
    assert!(borrowed.calibrated_model().function_evaluation() > 0);
    assert!(borrowed.calibrated_model().problem_values()[0].abs() < 1e-8);
}

struct TrialFailure {
    final_helper_failure: bool,
}

impl OptimizationMethod for TrialFailure {
    fn minimize(
        &mut self,
        problem: &mut Problem<'_>,
        _: &EndCriteria,
    ) -> QlResult<EndCriteriaType> {
        let trial = Array::from([3e-6, 0.06, 0.81, 0.12, 0.2, 0.0002]);
        assert!(problem.constraint().test(&trial));
        problem.values(&trial);
        problem.set_current_value(trial);
        if self.final_helper_failure {
            Ok(EndCriteriaType::StationaryPoint)
        } else {
            crate::fail!("deliberate optimizer failure after valid trial")
        }
    }
}

struct FailingHelper;

impl CalibrationHelper for FailingHelper {
    fn calibration_error(&mut self) -> QlResult<Real> {
        crate::fail!("deliberate helper failure")
    }
}

#[test]
fn failed_optimizer_and_final_helper_restore_parameters_and_process() {
    for final_helper_failure in [false, true] {
        let model = model();
        let helper = if final_helper_failure {
            shared_mut(FailingHelper) as SharedMut<dyn CalibrationHelper>
        } else {
            helper(&model)
        };
        let mut method = TrialFailure {
            final_helper_failure,
        };
        assert!(
            calibrate(
                &model,
                &[helper],
                &mut method,
                &criteria(),
                None,
                Vec::new(),
                Vec::new(),
            )
            .is_err()
        );
        let borrowed = model.borrow();
        assert_eq!(borrowed.calibrated_model().params(), array(parameters()));
        assert_eq!(borrowed.process().parameters(), parameters());
        assert_eq!(
            borrowed.calibrated_model().end_criteria(),
            EndCriteriaType::None
        );
        assert!(borrowed.calibrated_model().problem_values().is_empty());
        assert_eq!(borrowed.calibrated_model().function_evaluation(), 0);
    }
}

#[test]
fn model_constraint_checks_coupled_domains_and_does_not_capture_stale_parameters() {
    let model = model();
    let constraint = model.borrow().constraint();
    assert!(constraint.test(&array(parameters())));
    assert!(!constraint.test(&Array::from([2e-6, 0.7, 0.1, -0.2, -0.4, 0.0001])));
    assert!(!constraint.test(&Array::from([2e-6, 0.04, 0.8, -0.05, -0.4, 0.0001])));
    assert!(!constraint.test(&Array::from([2e-6, 0.04, 0.8, 0.05, Real::MAX, 0.0001])));
    let trial = Array::from([3e-6, 0.06, 0.81, 0.12, 0.2, 0.0002]);
    model.borrow_mut().set_params(&trial).unwrap();
    assert!(constraint.test(&trial));
}
