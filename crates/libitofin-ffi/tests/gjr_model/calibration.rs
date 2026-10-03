//! Independent QuantLib 1.43 numerical expectations are documented in
//! `sdk/go/testdata/gjrgarch-model-oracle.md` and its tracked JSON fixtures.
use super::*;
use itofin_ffi::boundary::{CORE_ERROR, INVALID_ARGUMENT};
use itofin_ffi::constraint_api::ItofinCalibrationOptions;
use itofin_ffi::gjr_model_api::calibration::itofin_gjr_calibrate_with_options;
use libitofin::handle::Handle;
use libitofin::math::array::Array;
use libitofin::math::optimization::{
    endcriteria::{EndCriteria, EndCriteriaType},
    method::OptimizationMethod,
    problem::Problem,
    simplex::Simplex,
};
use libitofin::models::calibrationhelper::CalibrationErrorType;
use libitofin::models::equity::HestonModelHelper;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::shared::{Shared, SharedMut, shared_mut};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::{calendars::nullcalendar::NullCalendar, period::Period, timeunit::TimeUnit};

fn setup(m: &mut Market, volatility: f64) -> (u64, u64, u64) {
    let settings = shared(Settings::<Date>::new());
    settings.set_evaluation_date(Date::new(3, Month::October, 2026));
    let helper = shared_mut(HestonModelHelper::new(
        Period::new(365, TimeUnit::Days),
        NullCalendar::new(),
        100.0,
        100.0,
        Handle::new(shared(SimpleQuote::new(volatility)) as Shared<dyn Quote>),
        m.context
            .get::<Handle<dyn YieldTermStructure>>(m.curves[0])
            .unwrap(),
        m.context
            .get::<Handle<dyn YieldTermStructure>>(m.curves[1])
            .unwrap(),
        CalibrationErrorType::RelativePriceError,
        settings,
    ));
    let method: SharedMut<dyn OptimizationMethod> = shared_mut(Simplex::new(1e-5));
    (
        m.context.insert(helper).unwrap(),
        m.context.insert(method).unwrap(),
        m.context
            .insert(EndCriteria::new(1000, Some(50), 1e-12, 1e-12, None).unwrap())
            .unwrap(),
    )
}

struct TrialFailure;
impl OptimizationMethod for TrialFailure {
    fn minimize(
        &mut self,
        problem: &mut Problem<'_>,
        _: &EndCriteria,
    ) -> libitofin::errors::QlResult<EndCriteriaType> {
        let trial = Array::from([0.0002]);
        problem.values(&trial);
        problem.set_current_value(trial);
        libitofin::fail!("deliberate optimizer failure after changing the model")
    }
}

#[test]
fn calibration_options_and_optimizer_failure_preserve_model() {
    let mut m = Market::new();
    let before = m.params();
    let (helper, method, criteria) = setup(&mut m, 0.2);
    let fixed = [1_u8, 1, 1, 1, 1, 0];
    let options = ItofinCalibrationOptions {
        constraint: 0,
        weights: std::ptr::null(),
        weights_len: 0,
        fix_parameters: fixed.as_ptr(),
        fix_parameters_len: 6,
    };
    let failing: SharedMut<dyn OptimizationMethod> = shared_mut(TrialFailure);
    let failing = m.context.insert(failing).unwrap();
    assert_eq!(
        unsafe {
            itofin_gjr_calibrate_with_options(
                &mut m.context,
                m.model,
                &helper,
                1,
                failing,
                criteria,
                &options,
                null_mut(),
            )
        },
        CORE_ERROR
    );
    assert_eq!(m.params(), before);
    for bad_weight in [-1.0, f64::NAN, f64::INFINITY] {
        let invalid = ItofinCalibrationOptions {
            weights: &bad_weight,
            weights_len: 1,
            ..options
        };
        assert_eq!(
            unsafe {
                itofin_gjr_calibrate_with_options(
                    &mut m.context,
                    m.model,
                    &helper,
                    1,
                    method,
                    criteria,
                    &invalid,
                    null_mut(),
                )
            },
            CORE_ERROR
        );
        assert_eq!(m.params(), before);
    }
    let bad_flags = [2_u8; 6];
    let invalid = ItofinCalibrationOptions {
        fix_parameters: bad_flags.as_ptr(),
        ..options
    };
    assert_eq!(
        unsafe {
            itofin_gjr_calibrate_with_options(
                &mut m.context,
                m.model,
                &helper,
                1,
                method,
                criteria,
                &invalid,
                null_mut(),
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(
        unsafe {
            itofin_gjr_calibrate_with_options(
                &mut m.context,
                m.model,
                std::ptr::null(),
                0,
                method,
                criteria,
                &options,
                null_mut(),
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(m.params(), before);
}

#[test]
fn weighted_fixed_mask_calibration_matches_independent_quantlib_quotes() {
    use itofin_ffi::constraint_api::itofin_no_constraint_new;
    use libitofin::models::{CalibratedModelHolder, GjrGarchModel};
    let mut m = Market::new();
    assert_eq!(
        unsafe { itofin_quote_set(&mut m.context, m.quotes[2], 0.0, null_mut()) },
        0
    );
    let mut start = m.params();
    start[5] *= 0.7;
    assert_eq!(
        unsafe {
            itofin_gjr_model_set_params(&mut m.context, m.model, start.as_ptr(), 6, null_mut())
        },
        0
    );
    let settings = shared(Settings::<Date>::new());
    settings.set_evaluation_date(Date::new(3, Month::October, 2026));
    let rows = [
        (14, 90.0, 0.2179813606260463),
        (14, 110.0, 0.20084652082605864),
        (35, 90.0, 0.1742514366431745),
        (35, 110.0, 0.16993201995496995),
        (63, 90.0, 0.1518589783473844),
        (63, 110.0, 0.1482819224412038),
    ];
    let mut helpers = Vec::new();
    for (days, strike, volatility) in rows {
        let helper = shared_mut(HestonModelHelper::new(
            Period::new(days, TimeUnit::Days),
            NullCalendar::new(),
            100.0,
            strike,
            Handle::new(shared(SimpleQuote::new(volatility)) as Shared<dyn Quote>),
            m.context
                .get::<Handle<dyn YieldTermStructure>>(m.curves[0])
                .unwrap(),
            m.context
                .get::<Handle<dyn YieldTermStructure>>(m.curves[1])
                .unwrap(),
            CalibrationErrorType::ImpliedVolError,
            settings.clone(),
        ));
        helpers.push(m.context.insert(helper).unwrap());
    }
    let method: SharedMut<dyn OptimizationMethod> = shared_mut(Simplex::new(2e-5));
    let method = m.context.insert(method).unwrap();
    let criteria = m
        .context
        .insert(EndCriteria::new(500, Some(40), 1e-10, 1e-10, Some(1e-10)).unwrap())
        .unwrap();
    let fixed = [1_u8, 1, 1, 1, 1, 0];
    let weights = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    let mut constraint = 0;
    assert_eq!(
        unsafe { itofin_no_constraint_new(&mut m.context, &mut constraint, null_mut()) },
        0
    );
    let options = ItofinCalibrationOptions {
        constraint,
        weights: weights.as_ptr(),
        weights_len: 6,
        fix_parameters: fixed.as_ptr(),
        fix_parameters_len: 6,
    };
    let mut error = itofin_ffi::ItofinError {
        code: 0,
        message: [0; 1024],
    };
    assert_eq!(
        unsafe {
            itofin_gjr_calibrate_with_options(
                &mut m.context,
                m.model,
                helpers.as_ptr(),
                6,
                method,
                criteria,
                &options,
                &mut error,
            )
        },
        0,
        "{:?}",
        error.message
    );
    let fitted = m.params();
    assert_eq!(fitted[..5], start[..5]);
    assert!((fitted[5] - 0.00015873012966579863).abs() < 1e-12);
    let retained = m.context.get::<SharedMut<GjrGarchModel>>(m.model).unwrap();
    let borrowed = retained.borrow();
    let metadata = (
        borrowed.calibrated_model().end_criteria(),
        borrowed.calibrated_model().function_evaluation(),
        borrowed.calibrated_model().problem_values().clone(),
    );
    assert!(metadata.0.succeeded());
    assert!(metadata.1 > 0);
    assert!(metadata.2.iter().map(|x| x * x).sum::<f64>() < 1e-12);
    drop(borrowed);
    let failing: SharedMut<dyn OptimizationMethod> = shared_mut(TrialFailure);
    let failing = m.context.insert(failing).unwrap();
    assert_eq!(
        unsafe {
            itofin_gjr_calibrate_with_options(
                &mut m.context,
                m.model,
                helpers.as_ptr(),
                6,
                failing,
                criteria,
                &options,
                null_mut(),
            )
        },
        CORE_ERROR
    );
    assert_eq!(m.params(), fitted);
    let borrowed = retained.borrow();
    assert_eq!(borrowed.calibrated_model().end_criteria(), metadata.0);
    assert_eq!(
        borrowed.calibrated_model().function_evaluation(),
        metadata.1
    );
    assert_eq!(borrowed.calibrated_model().problem_values(), &metadata.2);
}
