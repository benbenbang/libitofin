//! Native model handles and calibration adapters.
use crate::boundary::*;
use crate::time_api::{date, day_counter};
use libitofin::handle::Handle;
use libitofin::interestrate::Compounding;
use libitofin::math::optimization::{
    conjugategradient::ConjugateGradient,
    endcriteria::{EndCriteria, EndCriteriaType},
    levenbergmarquardt::LevenbergMarquardt,
    method::OptimizationMethod,
    simplex::Simplex,
    steepestdescent::SteepestDescent,
};
use libitofin::models::calibrationhelper::{BlackCalibrationHelper, CalibrationHelper};
use libitofin::models::equity::HestonModelHelper;
use libitofin::models::shortrate::SwaptionHelper;
use libitofin::models::{
    CalibratedModelHolder, CalibrationErrorType, HestonModel, HullWhite, calibrate,
};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::JamshidianSwaptionEngine;
use libitofin::pricingengines::vanilla::analytichestonengine::AnalyticHestonEngine;
use libitofin::processes::HestonProcess;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::frequency::Frequency;
use libitofin::time::{date::Date, daycounter::DayCounter};

pub(crate) fn error_type(value: i32) -> BindingResult<CalibrationErrorType> {
    match value {
        0 => Ok(CalibrationErrorType::RelativePriceError),
        1 => Ok(CalibrationErrorType::PriceError),
        2 => Ok(CalibrationErrorType::ImpliedVolError),
        _ => Err(BindingError::invalid("unknown calibration error type")),
    }
}
fn flat(reference: Date, rate: f64, dc: DayCounter) -> Handle<dyn YieldTermStructure> {
    Handle::new(shared(FlatForward::with_rate(
        reference,
        rate,
        dc,
        Compounding::Continuous,
        Frequency::Annual,
    )) as Shared<dyn YieldTermStructure>)
}

/// Scalar market inputs followed by native Heston parameters.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HestonProcessConfig {
    pub risk_free_rate: f64,
    pub dividend_yield: f64,
    pub spot: f64,
    pub v0: f64,
    pub kappa: f64,
    pub theta: f64,
    pub sigma: f64,
    pub rho: f64,
    pub reference_date: i32,
    pub day_counter: u64,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_heston_process_new(
    ctx: *mut Context,
    config: HestonProcessConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let d = date(config.reference_date)?;
            let dc = day_counter(c, config.day_counter)?;
            let process = HestonProcess::new(
                flat(d, config.risk_free_rate, dc.clone()),
                flat(d, config.dividend_yield, dc),
                Handle::new(shared(SimpleQuote::new(config.spot)) as Shared<dyn Quote>),
                config.v0,
                config.kappa,
                config.theta,
                config.sigma,
                config.rho,
            );
            output(out, c.insert(shared(process))?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_heston_model_new(
    ctx: *mut Context,
    process: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let model = HestonModel::new(c.get::<Shared<HestonProcess>>(process)?)?;
            output(out, c.insert(model)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_hullwhite_new(
    ctx: *mut Context,
    curve: u64,
    a: f64,
    sigma: f64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let model = HullWhite::new(c.get::<Handle<dyn YieldTermStructure>>(curve)?, a, sigma)?;
            output(out, c.insert(model)?)
        })
    }
}

/// Kind 0 Heston process, 1 Heston model, 2 HullWhite.
/// Heston fields: v0, kappa, theta, sigma, rho. HullWhite fields: a, sigma, r0.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_model_parameter(
    ctx: *mut Context,
    handle: u64,
    kind: i32,
    field: usize,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let values = match kind {
                0 => {
                    let p = c.get::<Shared<HestonProcess>>(handle)?;
                    vec![p.v0(), p.kappa(), p.theta(), p.sigma(), p.rho()]
                }
                1 => {
                    let p = c.get::<SharedMut<HestonModel>>(handle)?;
                    let p = p.borrow();
                    vec![p.v0(), p.kappa(), p.theta(), p.sigma(), p.rho()]
                }
                2 => {
                    let p = c.get::<SharedMut<HullWhite>>(handle)?;
                    let p = p.borrow();
                    let v = p.calibrated_model().params();
                    vec![v[0], v[1], p.r0()]
                }
                _ => return Err(BindingError::invalid("unknown model kind")),
            };
            output(
                out,
                *values
                    .get(field)
                    .ok_or_else(|| BindingError::invalid("unknown model field"))?,
            )
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_hullwhite_bond_option(
    ctx: *mut Context,
    model: u64,
    kind: i32,
    strike: f64,
    maturity: f64,
    bond_maturity: f64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let model = c.get::<SharedMut<HullWhite>>(model)?;
            let value = model.borrow().discount_bond_option(
                crate::options_api::option_type(kind)?,
                strike,
                maturity,
                bond_maturity,
            )?;
            output(out, value)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_levenberg_marquardt_new(
    ctx: *mut Context,
    epsfcn: f64,
    xtol: f64,
    gtol: f64,
    jacobian: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if !(0..=1).contains(&jacobian) {
                return Err(BindingError::invalid("jacobian must be 0 or 1"));
            }
            output(
                out,
                c.insert(
                    shared_mut(LevenbergMarquardt::new(epsfcn, xtol, gtol, jacobian != 0))
                        as SharedMut<dyn OptimizationMethod>,
                )?,
            )
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid. Context and handles belong to
/// the calling thread; serialize calls including destruction.
pub unsafe extern "C" fn itofin_simplex_new(
    ctx: *mut Context,
    lambda: f64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if !lambda.is_finite() || lambda <= 0.0 {
                return Err(BindingError::invalid(
                    "simplex lambda must be finite and positive",
                ));
            }
            output(
                out,
                c.insert(shared_mut(Simplex::new(lambda)) as SharedMut<dyn OptimizationMethod>)?,
            )
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid. Context and handles belong to
/// the calling thread; serialize calls including destruction.
pub unsafe extern "C" fn itofin_conjugate_gradient_new(
    ctx: *mut Context,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            output(out, c.insert(shared_mut(ConjugateGradient::new()) as SharedMut<dyn OptimizationMethod>)?)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid. Context and handles belong to
/// the calling thread; serialize calls including destruction.
pub unsafe extern "C" fn itofin_steepest_descent_new(
    ctx: *mut Context,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            output(
                out,
                c.insert(shared_mut(SteepestDescent::new()) as SharedMut<dyn OptimizationMethod>)?,
            )
        })
    }
}
/// Presence flags distinguish omitted criteria from explicit zero values.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct EndCriteriaConfig {
    pub max_iterations: usize,
    pub stationary_iterations: usize,
    pub root_epsilon: f64,
    pub function_epsilon: f64,
    pub gradient_epsilon: f64,
    pub has_stationary: i32,
    pub has_gradient: i32,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_end_criteria_new(
    ctx: *mut Context,
    config: EndCriteriaConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if !(0..=1).contains(&config.has_stationary) || !(0..=1).contains(&config.has_gradient)
            {
                return Err(BindingError::invalid("presence flags must be 0 or 1"));
            }
            let end = EndCriteria::new(
                config.max_iterations,
                (config.has_stationary != 0).then_some(config.stationary_iterations),
                config.root_epsilon,
                config.function_epsilon,
                (config.has_gradient != 0).then_some(config.gradient_epsilon),
            )?;
            output(out, c.insert(end)?)
        })
    }
}

/// Maturity is a value (length, Python TimeUnit discriminant).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HestonHelperConfig {
    pub maturity_length: i32,
    pub maturity_unit: i32,
    pub calendar: u64,
    pub spot: f64,
    pub strike: f64,
    pub volatility: f64,
    pub risk_free_rate: f64,
    pub dividend_yield: f64,
    pub error_type: i32,
    pub reference_date: i32,
    pub day_counter: u64,
    pub settings: u64,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_heston_helper_new(
    ctx: *mut Context,
    cfg: HestonHelperConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let d = date(cfg.reference_date)?;
            let dc = day_counter(c, cfg.day_counter)?;
            let helper = HestonModelHelper::new(
                libitofin::time::period::Period::new(
                    cfg.maturity_length,
                    crate::time_api::time_unit(cfg.maturity_unit)?,
                ),
                crate::time_api::calendar(c, cfg.calendar)?,
                cfg.spot,
                cfg.strike,
                Handle::new(shared(SimpleQuote::new(cfg.volatility)) as Shared<dyn Quote>),
                flat(d, cfg.risk_free_rate, dc.clone()),
                flat(d, cfg.dividend_yield, dc),
                error_type(cfg.error_type)?,
                crate::settings_api::settings(c, cfg.settings)?,
            );
            output(out, c.insert(shared_mut(helper))?)
        })
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwaptionHelperConfig {
    pub maturity_length: i32,
    pub maturity_unit: i32,
    pub length: i32,
    pub length_unit: i32,
    pub volatility: f64,
    pub index: u64,
    pub fixed_tenor_length: i32,
    pub fixed_tenor_unit: i32,
    pub fixed_day_counter: u64,
    pub floating_day_counter: u64,
    pub curve: u64,
    pub error_type: i32,
    pub nominal: f64,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_swaption_helper_new(
    ctx: *mut Context,
    cfg: SwaptionHelperConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            use libitofin::time::period::Period;
            let helper = SwaptionHelper::new(
                Period::new(
                    cfg.maturity_length,
                    crate::time_api::time_unit(cfg.maturity_unit)?,
                ),
                Period::new(cfg.length, crate::time_api::time_unit(cfg.length_unit)?),
                Handle::new(shared(SimpleQuote::new(cfg.volatility)) as Shared<dyn Quote>),
                crate::indexes_api::ibor_index(c, cfg.index)?,
                Period::new(
                    cfg.fixed_tenor_length,
                    crate::time_api::time_unit(cfg.fixed_tenor_unit)?,
                ),
                day_counter(c, cfg.fixed_day_counter)?,
                day_counter(c, cfg.floating_day_counter)?,
                c.get::<Handle<dyn YieldTermStructure>>(cfg.curve)?,
                error_type(cfg.error_type)?,
                None,
                cfg.nominal,
                libitofin::termstructures::volatility::VolatilityType::ShiftedLognormal,
                0.0,
                None,
                libitofin::cashflows::RateAveraging::Compound,
            );
            output(out, c.insert(shared_mut(helper))?)
        })
    }
}
/// Kind 0 Heston helper, 1 swaption helper.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_helper_error(
    ctx: *mut Context,
    helper: u64,
    kind: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let value = match kind {
                0 => c
                    .get::<SharedMut<HestonModelHelper>>(helper)?
                    .borrow_mut()
                    .calibration_error()?,
                1 => c
                    .get::<SharedMut<SwaptionHelper>>(helper)?
                    .borrow_mut()
                    .calibration_error()?,
                _ => return Err(BindingError::invalid("unknown helper kind")),
            };
            output(out, value)
        })
    }
}
/// Calibration kind 0 Heston (integration order), 1 HullWhite (fix_reversion).
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_model_calibrate(
    ctx: *mut Context,
    model: u64,
    kind: i32,
    helpers: *const u64,
    helpers_len: usize,
    method: u64,
    criteria: u64,
    integration_order: usize,
    fix_reversion: i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let ids = input_slice(helpers, helpers_len)?;
            if ids.is_empty() {
                return Err(BindingError::invalid(
                    "calibration helpers must not be empty",
                ));
            }
            let method = c.get::<SharedMut<dyn OptimizationMethod>>(method)?;
            let criteria = c.get::<EndCriteria>(criteria)?;
            match kind {
                0 => {
                    let model = c.get::<SharedMut<HestonModel>>(model)?;
                    let helpers = ids
                        .iter()
                        .map(|id| c.get::<SharedMut<HestonModelHelper>>(*id))
                        .collect::<BindingResult<Vec<_>>>()?;
                    let engine =
                        shared_mut(AnalyticHestonEngine::new(model.clone(), integration_order)?)
                            as SharedMut<dyn PricingEngine>;
                    for helper in &helpers {
                        helper
                            .borrow_mut()
                            .base_mut()
                            .set_pricing_engine(engine.clone());
                    }
                    let helpers: Vec<SharedMut<dyn CalibrationHelper>> = helpers
                        .into_iter()
                        .map(|h| h as SharedMut<dyn CalibrationHelper>)
                        .collect();
                    calibrate(
                        &model,
                        &helpers,
                        &mut *method.borrow_mut(),
                        &criteria,
                        None,
                        vec![],
                        vec![],
                    )?;
                }
                1 => {
                    if !(0..=1).contains(&fix_reversion) {
                        return Err(BindingError::invalid("fix_reversion must be 0 or 1"));
                    }
                    let model = c.get::<SharedMut<HullWhite>>(model)?;
                    let helpers = ids
                        .iter()
                        .map(|id| c.get::<SharedMut<SwaptionHelper>>(*id))
                        .collect::<BindingResult<Vec<_>>>()?;
                    let engine = shared_mut(JamshidianSwaptionEngine::new(model.clone()))
                        as SharedMut<dyn PricingEngine>;
                    for helper in &helpers {
                        helper
                            .borrow_mut()
                            .base_mut()
                            .set_pricing_engine(engine.clone());
                    }
                    let helpers: Vec<SharedMut<dyn CalibrationHelper>> = helpers
                        .into_iter()
                        .map(|h| h as SharedMut<dyn CalibrationHelper>)
                        .collect();
                    let fixed = if fix_reversion != 0 {
                        vec![true, false]
                    } else {
                        vec![]
                    };
                    calibrate(
                        &model,
                        &helpers,
                        &mut *method.borrow_mut(),
                        &criteria,
                        None,
                        vec![],
                        fixed,
                    )?;
                }
                _ => return Err(BindingError::invalid("unknown calibration kind")),
            }
            Ok(())
        })
    }
}

#[unsafe(no_mangle)]
/// Calibration kind 0 is Heston and 1 is Hull-White. Result codes follow
/// `EndCriteriaType`: None 0, MaxIterations 1, StationaryPoint 2,
/// StationaryFunctionValue 3, StationaryFunctionAccuracy 4,
/// ZeroGradientNorm 5, FunctionEpsilonTooSmall 6, Unknown 7.
/// # Safety
/// Pointers must be aligned, live and valid. Context and handles belong to
/// the calling thread; serialize calls including destruction.
pub unsafe extern "C" fn itofin_model_end_criteria_type(
    ctx: *mut Context,
    model: u64,
    kind: i32,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let result = match kind {
                0 => c
                    .get::<SharedMut<HestonModel>>(model)?
                    .borrow()
                    .calibrated_model()
                    .end_criteria(),
                1 => c
                    .get::<SharedMut<HullWhite>>(model)?
                    .borrow()
                    .calibrated_model()
                    .end_criteria(),
                _ => return Err(BindingError::invalid("unknown calibration kind")),
            };
            let code = match result {
                EndCriteriaType::None => 0,
                EndCriteriaType::MaxIterations => 1,
                EndCriteriaType::StationaryPoint => 2,
                EndCriteriaType::StationaryFunctionValue => 3,
                EndCriteriaType::StationaryFunctionAccuracy => 4,
                EndCriteriaType::ZeroGradientNorm => 5,
                EndCriteriaType::FunctionEpsilonTooSmall => 6,
                EndCriteriaType::Unknown => 7,
            };
            output(out, code)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libitofin::math::array::Array;
    use libitofin::math::optimization::constraint::NoConstraint;
    use libitofin::math::optimization::costfunction::CostFunction;
    use libitofin::math::optimization::problem::Problem;
    use std::ptr::null_mut;
    #[test]
    fn calibration_enum_and_optional_criteria_are_checked() {
        assert_eq!(
            error_type(0).unwrap(),
            CalibrationErrorType::RelativePriceError
        );
        assert_eq!(error_type(1).unwrap(), CalibrationErrorType::PriceError);
        assert_eq!(
            error_type(2).unwrap(),
            CalibrationErrorType::ImpliedVolError
        );
        assert!(error_type(3).is_err());
        let mut c = Context::new();
        let mut id = 99;
        let mut config = EndCriteriaConfig {
            max_iterations: 100,
            stationary_iterations: 0,
            root_epsilon: 1e-8,
            function_epsilon: 1e-8,
            gradient_epsilon: 0.,
            has_stationary: 0,
            has_gradient: 0,
        };
        unsafe {
            assert_eq!(
                itofin_end_criteria_new(&mut c, config, &mut id, null_mut()),
                0
            );
            config.has_stationary = 1;
            assert_eq!(
                itofin_end_criteria_new(&mut c, config, &mut id, null_mut()),
                CORE_ERROR
            );
            config.has_stationary = 2;
            assert_eq!(
                itofin_end_criteria_new(&mut c, config, &mut id, null_mut()),
                INVALID_ARGUMENT
            );
        }
    }

    #[test]
    fn optimization_methods_share_one_handle_kind() {
        let mut c = Context::new();
        let mut id = 0;
        unsafe {
            assert_eq!(
                itofin_levenberg_marquardt_new(&mut c, 1e-8, 1e-8, 1e-8, 0, &mut id, null_mut()),
                0
            );
            assert!(c.get::<SharedMut<dyn OptimizationMethod>>(id).is_ok());
            assert_eq!(itofin_simplex_new(&mut c, 0.1, &mut id, null_mut()), 0);
            assert!(c.get::<SharedMut<dyn OptimizationMethod>>(id).is_ok());
            assert_eq!(
                itofin_conjugate_gradient_new(&mut c, &mut id, null_mut()),
                0
            );
            assert!(c.get::<SharedMut<dyn OptimizationMethod>>(id).is_ok());
            assert_eq!(itofin_steepest_descent_new(&mut c, &mut id, null_mut()), 0);
            assert!(c.get::<SharedMut<dyn OptimizationMethod>>(id).is_ok());
            for lambda in [0.0, -1.0, f64::NAN, f64::INFINITY] {
                assert_eq!(
                    itofin_simplex_new(&mut c, lambda, &mut id, null_mut()),
                    INVALID_ARGUMENT
                );
            }
        }
    }

    #[test]
    fn ffi_method_handles_match_core_parabola_oracles() {
        struct Parabola;
        impl CostFunction for Parabola {
            fn values(&self, x: &Array) -> Array {
                Array::from([x[0] * x[0] + x[0] + 1.0])
            }
        }
        let mut c = Context::new();
        let mut id = 0;
        let mut methods: Vec<(&str, u64, Box<dyn OptimizationMethod>)> = Vec::new();
        unsafe {
            assert_eq!(itofin_simplex_new(&mut c, 0.1, &mut id, null_mut()), 0);
            methods.push(("simplex", id, Box::new(Simplex::new(0.1))));
            assert_eq!(
                itofin_conjugate_gradient_new(&mut c, &mut id, null_mut()),
                0
            );
            methods.push(("conjugate gradient", id, Box::new(ConjugateGradient::new())));
            assert_eq!(itofin_steepest_descent_new(&mut c, &mut id, null_mut()), 0);
            methods.push(("steepest descent", id, Box::new(SteepestDescent::new())));
        }
        let cost = Parabola;
        let constraint = NoConstraint;
        let criteria = EndCriteria::new(10_000, Some(100), 1e-8, 1e-8, Some(1e-8)).unwrap();
        for (name, id, mut core) in methods {
            let mut direct = Problem::new(&cost, &constraint, Array::from([-100.0]));
            let mut bridged = Problem::new(&cost, &constraint, Array::from([-100.0]));
            let direct_reason = core.minimize(&mut direct, &criteria).unwrap();
            let handle = c.get::<SharedMut<dyn OptimizationMethod>>(id).unwrap();
            let bridged_reason = handle
                .borrow_mut()
                .minimize(&mut bridged, &criteria)
                .unwrap();
            assert_eq!(bridged_reason, direct_reason, "{name}");
            assert!(
                (bridged.current_value()[0] - direct.current_value()[0]).abs() <= 1e-12,
                "{name}"
            );
            assert!(
                (bridged.function_value() - direct.function_value()).abs() <= 1e-12,
                "{name}"
            );
            let x_error = (bridged.current_value()[0] + 0.5).abs();
            let y_error = (bridged.function_value() - 0.75).abs();
            assert!(x_error <= 1e-8 || y_error <= 1e-8, "{name}");
        }
    }

    #[test]
    fn ffi_heston_calibration_matches_core_in_same_profile() {
        let mut c = Context::new();
        let mut reference_date = 0;
        let mut dc = 0;
        let mut calendar = 0;
        let mut settings = 0;
        let mut process = 0;
        let mut criteria = 0;
        unsafe {
            assert_eq!(
                crate::time_api::itofin_date_new(15, 1, 2026, &mut reference_date, null_mut()),
                0
            );
            assert_eq!(
                crate::time_api::itofin_day_counter_new(&mut c, 0, &mut dc, null_mut()),
                0
            );
            assert_eq!(
                crate::time_api::itofin_calendar_new(&mut c, 1, &mut calendar, null_mut()),
                0
            );
            assert_eq!(
                crate::settings_api::itofin_settings_new(&mut c, &mut settings, null_mut()),
                0
            );
            assert_eq!(
                crate::settings_api::itofin_settings_set_evaluation_date(
                    &mut c,
                    settings,
                    reference_date,
                    null_mut(),
                ),
                0
            );
            assert_eq!(
                itofin_heston_process_new(
                    &mut c,
                    HestonProcessConfig {
                        risk_free_rate: 0.04,
                        dividend_yield: 0.50,
                        spot: 1.0,
                        v0: 0.01,
                        kappa: 0.2,
                        theta: 0.02,
                        sigma: 0.3,
                        rho: -0.75,
                        reference_date,
                        day_counter: dc,
                    },
                    &mut process,
                    null_mut(),
                ),
                0
            );
            assert_eq!(
                itofin_end_criteria_new(
                    &mut c,
                    EndCriteriaConfig {
                        max_iterations: 400,
                        stationary_iterations: 40,
                        root_epsilon: 1e-8,
                        function_epsilon: 1e-8,
                        gradient_epsilon: 1e-8,
                        has_stationary: 1,
                        has_gradient: 1,
                    },
                    &mut criteria,
                    null_mut(),
                ),
                0
            );
        }
        // Exact bits match the Go Heston fixture across optimization methods.
        let strikes = [
            [0x3fefac53e80821cf, 0x3feec1d93a138c3f, 0x3fedde266b06edcb],
            [0x3feee6fc4e5c066b, 0x3fedad1ea01315b6, 0x3fec7fb4cb280859],
            [0x3fedfc74e7ab4083, 0x3fec86124a9aed52, 0x3feb21f1fa31573d],
            [0x3feb422c40cceb6b, 0x3fe96481fc737590, 0x3fe7a78a19df52fa],
            [0x3fe8a167781bf00b, 0x3fe6938b2fef7da4, 0x3fe4b18a04b47ab2],
            [0x3fe632e5c3c88016, 0x3fe4128a7c687e73, 0x3fe22653d92d71bf],
            [0x3fdd08fc2bc78913, 0x3fd92e6fb34bcd0d, 0x3fd5d6d3fbc52310],
        ];
        let maturities = [(1, 2), (2, 2), (3, 2), (6, 2), (9, 2), (1, 3), (2, 3)];
        let mut helper_ids = Vec::new();
        for ((length, unit), row) in maturities.into_iter().zip(strikes) {
            for bits in row {
                let mut id = 0;
                let config = HestonHelperConfig {
                    maturity_length: length,
                    maturity_unit: unit,
                    calendar,
                    spot: 1.0,
                    strike: f64::from_bits(bits),
                    volatility: 0.1,
                    risk_free_rate: 0.04,
                    dividend_yield: 0.50,
                    error_type: 0,
                    reference_date,
                    day_counter: dc,
                    settings,
                };
                unsafe {
                    assert_eq!(
                        itofin_heston_helper_new(&mut c, config, &mut id, null_mut()),
                        0
                    );
                }
                helper_ids.push(id);
            }
        }
        for (name, constructor, mut direct_method) in [
            (
                "simplex",
                0,
                Box::new(Simplex::new(0.1)) as Box<dyn OptimizationMethod>,
            ),
            ("conjugate gradient", 1, Box::new(ConjugateGradient::new())),
            ("steepest descent", 2, Box::new(SteepestDescent::new())),
        ] {
            let mut bridged_model_id = 0;
            let mut method_id = 0;
            unsafe {
                assert_eq!(
                    itofin_heston_model_new(&mut c, process, &mut bridged_model_id, null_mut()),
                    0
                );
                let status = match constructor {
                    0 => itofin_simplex_new(&mut c, 0.1, &mut method_id, null_mut()),
                    1 => itofin_conjugate_gradient_new(&mut c, &mut method_id, null_mut()),
                    _ => itofin_steepest_descent_new(&mut c, &mut method_id, null_mut()),
                };
                assert_eq!(status, 0, "{name}");
                assert_eq!(
                    itofin_model_calibrate(
                        &mut c,
                        bridged_model_id,
                        0,
                        helper_ids.as_ptr(),
                        helper_ids.len(),
                        method_id,
                        criteria,
                        96,
                        0,
                        null_mut(),
                    ),
                    0,
                    "{name}"
                );
            }

            let core_model =
                HestonModel::new(c.get::<Shared<HestonProcess>>(process).unwrap()).unwrap();
            let engine = shared_mut(AnalyticHestonEngine::new(core_model.clone(), 96).unwrap())
                as SharedMut<dyn PricingEngine>;
            let helpers: Vec<SharedMut<dyn CalibrationHelper>> = helper_ids
                .iter()
                .map(|id| {
                    let helper = c.get::<SharedMut<HestonModelHelper>>(*id).unwrap();
                    helper
                        .borrow_mut()
                        .base_mut()
                        .set_pricing_engine(engine.clone());
                    helper as SharedMut<dyn CalibrationHelper>
                })
                .collect();
            calibrate(
                &core_model,
                &helpers,
                &mut *direct_method,
                &c.get::<EndCriteria>(criteria).unwrap(),
                None,
                vec![],
                vec![],
            )
            .unwrap();
            let core = core_model.borrow();
            let expected = [
                core.v0(),
                core.kappa(),
                core.theta(),
                core.sigma(),
                core.rho(),
            ];
            for (field, expected) in expected.into_iter().enumerate() {
                let mut actual = 0.0;
                unsafe {
                    assert_eq!(
                        itofin_model_parameter(
                            &mut c,
                            bridged_model_id,
                            1,
                            field,
                            &mut actual,
                            null_mut(),
                        ),
                        0,
                        "{name}"
                    );
                }
                assert!(
                    (actual - expected).abs() <= 1e-12,
                    "{name} field {field}: {actual} vs {expected}"
                );
            }
            let mut actual_reason = -1;
            unsafe {
                assert_eq!(
                    itofin_model_end_criteria_type(
                        &mut c,
                        bridged_model_id,
                        0,
                        &mut actual_reason,
                        null_mut(),
                    ),
                    0,
                    "{name}"
                );
            }
            assert_eq!(
                actual_reason,
                core.calibrated_model().end_criteria() as i32,
                "{name}"
            );
        }
    }
}
