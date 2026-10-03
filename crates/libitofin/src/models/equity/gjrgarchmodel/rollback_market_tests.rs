use super::tests::parameters;
use super::*;
use crate::exercise::EuropeanExercise;
use crate::handle::{Handle, RelinkableHandle};
use crate::instrument::Instrument;
use crate::instruments::{PlainVanillaPayoff, VanillaOption};
use crate::interestrate::Compounding;
use crate::math::optimization::endcriteria::{EndCriteria, EndCriteriaType};
use crate::math::optimization::method::OptimizationMethod;
use crate::math::optimization::problem::Problem;
use crate::math::optimization::simplex::Simplex;
use crate::models::calibrate;
use crate::models::calibrationhelper::CalibrationHelper;
use crate::option::OptionType;
use crate::pricingengines::vanilla::AnalyticGjrGarchEngine;
use crate::quotes::{Quote, SimpleQuote};
use crate::settings::Settings;
use crate::termstructures::yields::FlatForward;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::test_support::{Flag, as_observer};
use crate::time::{
    date::{Date, Month},
    daycounters::actual360::Actual360,
    frequency::Frequency,
};

#[derive(Clone)]
enum Market {
    Spot(Shared<SimpleQuote>),
    Curve(RelinkableHandle<dyn YieldTermStructure>),
}

fn reference() -> Date {
    Date::new(15, Month::June, 2026)
}

fn curve(date: Date) -> Shared<dyn YieldTermStructure> {
    shared(FlatForward::with_rate(
        date,
        0.03,
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ))
}

impl Market {
    fn invalidate(&self) {
        match self {
            Self::Spot(spot) => {
                spot.reset();
            }
            Self::Curve(curve_handle) => curve_handle.link_to(curve(Date::null())),
        }
    }

    fn repair(&self) {
        match self {
            Self::Spot(spot) => {
                spot.set_value(100.0);
            }
            Self::Curve(curve_handle) => curve_handle.link_to(curve(reference())),
        }
    }

    fn assert_invalid(&self, model: &SharedMut<GjrGarchModel>) {
        match self {
            Self::Spot(spot) => {
                assert!(spot.value().is_err());
                assert!(model.borrow().process().initial_values().is_err());
            }
            Self::Curve(curve_handle) => {
                assert!(
                    curve_handle
                        .handle()
                        .current_link()
                        .unwrap()
                        .reference_date()
                        .is_err()
                );
                assert!(model.borrow().process().time(&(reference() + 90)).is_err());
            }
        }
    }
}

fn fixture(invalidate_curve: bool) -> (SharedMut<GjrGarchModel>, Market) {
    let spot = shared(SimpleQuote::new(100.0));
    let risk_free = RelinkableHandle::new(curve(reference()));
    let process = shared(
        GjrGarchProcess::new(
            risk_free.handle(),
            Handle::new(curve(reference())),
            Handle::new(Shared::clone(&spot) as Shared<dyn Quote>),
            parameters(),
            GjrGarchDiscretization::FullTruncation,
        )
        .unwrap(),
    );
    let market = if invalidate_curve {
        Market::Curve(risk_free)
    } else {
        Market::Spot(spot)
    };
    (GjrGarchModel::new(process).unwrap(), market)
}

fn price(model: &SharedMut<GjrGarchModel>) -> QlResult<Real> {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(reference());
    let mut option = VanillaOption::new(
        shared(PlainVanillaPayoff::new(OptionType::Call, 100.0)),
        shared(EuropeanExercise::new(reference() + 90)),
        settings,
    );
    option
        .base_mut()
        .set_pricing_engine(shared_mut(AnalyticGjrGarchEngine::new(SharedMut::clone(
            model,
        ))));
    option.npv()
}

struct VarianceHelper {
    model: SharedMut<GjrGarchModel>,
    invalidate_after_first_call: Option<Market>,
    calls: usize,
}

impl CalibrationHelper for VarianceHelper {
    fn calibration_error(&mut self) -> QlResult<Real> {
        self.calls += 1;
        if self.calls > 1
            && let Some(market) = &self.invalidate_after_first_call
        {
            market.invalidate();
            crate::fail!("deliberate final helper failure after market invalidation");
        }
        Ok(self.model.borrow().process().initial_values()?[1] - 0.12)
    }
}

fn helper(
    model: &SharedMut<GjrGarchModel>,
    market: Option<Market>,
) -> SharedMut<dyn CalibrationHelper> {
    shared_mut(VarianceHelper {
        model: SharedMut::clone(model),
        invalidate_after_first_call: market,
        calls: 0,
    })
}

fn criteria() -> EndCriteria {
    EndCriteria::new(1000, Some(50), 1e-12, 1e-12, None).unwrap()
}

struct FailureAfterTrial {
    model: SharedMut<GjrGarchModel>,
    invalidate_before_optimizer_error: Option<Market>,
}

impl OptimizationMethod for FailureAfterTrial {
    fn minimize(
        &mut self,
        problem: &mut Problem<'_>,
        _: &EndCriteria,
    ) -> QlResult<EndCriteriaType> {
        let trial = Array::from([3e-6, 0.06, 0.81, 0.12, 0.2, 0.0002]);
        let values = problem.values(&trial);
        assert!(values.iter().all(|value| value.is_finite()));
        assert_eq!(self.model.borrow().calibrated_model().params(), trial);
        problem.set_current_value(trial);
        if let Some(market) = &self.invalidate_before_optimizer_error {
            market.invalidate();
            crate::fail!("deliberate optimizer failure after market invalidation");
        }
        Ok(EndCriteriaType::StationaryPoint)
    }
}

fn assert_market_failure_restores_snapshot(invalidate_curve: bool, fail_optimizer: bool) {
    let (model, market) = fixture(invalidate_curve);
    calibrate(
        &model,
        &[helper(&model, None)],
        &mut Simplex::new(0.0001),
        &criteria(),
        None,
        Vec::new(),
        vec![true, true, true, true, true, false],
    )
    .unwrap();
    let (params, process, end_criteria, residuals, evaluations) = {
        let borrowed = model.borrow();
        let calibrated = borrowed.calibrated_model();
        assert!(calibrated.end_criteria().succeeded());
        assert!(!calibrated.problem_values().is_empty());
        assert!(calibrated.function_evaluation() > 0);
        (
            calibrated.params(),
            borrowed.process(),
            calibrated.end_criteria(),
            calibrated.problem_values().clone(),
            calibrated.function_evaluation(),
        )
    };
    let initial_price = price(&model).unwrap();
    let flag = Flag::new();
    model
        .borrow()
        .observable()
        .register_observer(&as_observer(&flag));
    let mut method = FailureAfterTrial {
        model: SharedMut::clone(&model),
        invalidate_before_optimizer_error: fail_optimizer.then(|| market.clone()),
    };
    let helper = helper(&model, (!fail_optimizer).then(|| market.clone()));
    let error = calibrate(
        &model,
        &[helper],
        &mut method,
        &criteria(),
        None,
        Vec::new(),
        Vec::new(),
    )
    .unwrap_err();
    let expected = if fail_optimizer {
        "deliberate optimizer failure after market invalidation"
    } else {
        "nonfinite final calibration residuals"
    };
    assert_eq!(error.message(), expected);
    {
        let borrowed = model.borrow();
        let calibrated = borrowed.calibrated_model();
        assert_eq!(calibrated.params(), params);
        assert!(Shared::ptr_eq(&borrowed.process(), &process));
        assert_eq!(calibrated.end_criteria(), end_criteria);
        assert_eq!(calibrated.problem_values(), &residuals);
        assert_eq!(calibrated.function_evaluation(), evaluations);
    }
    assert!(Flag::is_up(&flag));
    market.assert_invalid(&model);
    assert!(price(&model).is_err());
    market.repair();
    assert_eq!(model.borrow().calibrated_model().params(), params);
    let repaired_price = price(&model).unwrap();
    assert!((repaired_price - initial_price).abs() < 1e-12);
}

#[test]
fn optimizer_failure_restores_model_snapshot_without_restoring_invalid_spot() {
    assert_market_failure_restores_snapshot(false, true);
}

#[test]
fn final_helper_failure_restores_model_snapshot_without_restoring_invalid_spot() {
    assert_market_failure_restores_snapshot(false, false);
}

#[test]
fn optimizer_failure_restores_model_snapshot_without_restoring_invalid_curve() {
    assert_market_failure_restores_snapshot(true, true);
}

#[test]
fn final_helper_failure_restores_model_snapshot_without_restoring_invalid_curve() {
    assert_market_failure_restores_snapshot(true, false);
}
