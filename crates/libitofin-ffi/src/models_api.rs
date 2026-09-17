//! Native model handles and calibration adapters.
use crate::boundary::*;
use crate::time_api::{date, day_counter};
use libitofin::handle::Handle;
use libitofin::interestrate::Compounding;
use libitofin::math::optimization::{
    endcriteria::EndCriteria, levenbergmarquardt::LevenbergMarquardt,
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
                c.insert(shared_mut(LevenbergMarquardt::new(
                    epsfcn,
                    xtol,
                    gtol,
                    jacobian != 0,
                )))?,
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
            let method = c.get::<SharedMut<LevenbergMarquardt>>(method)?;
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

#[cfg(test)]
mod tests {
    use super::*;
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
}
