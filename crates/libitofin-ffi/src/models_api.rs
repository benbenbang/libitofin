//! Native model handles and calibration adapters.
use crate::boundary::*;
use crate::time_api::{date, day_counter};
use libitofin::handle::Handle;
use libitofin::interestrate::Compounding;
use libitofin::math::optimization::{endcriteria::EndCriteria, levenbergmarquardt::LevenbergMarquardt};
use libitofin::models::{CalibratedModelHolder, CalibrationErrorType, HestonModel, HullWhite, calibrate};
use libitofin::models::calibrationhelper::{BlackCalibrationHelper, CalibrationHelper};
use libitofin::models::equity::HestonModelHelper;
use libitofin::models::shortrate::SwaptionHelper;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::JamshidianSwaptionEngine;
use libitofin::pricingengines::vanilla::analytichestonengine::AnalyticHestonEngine;
use libitofin::processes::HestonProcess;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::{Date, DayCounter};
use libitofin::time::frequency::Frequency;

pub(crate) fn error_type(value: i32) -> BindingResult<CalibrationErrorType> {
    match value { 0 => Ok(CalibrationErrorType::RelativePriceError),
        1 => Ok(CalibrationErrorType::PriceError), 2 => Ok(CalibrationErrorType::ImpliedVolError),
        _ => Err(BindingError::invalid("unknown calibration error type")) }
}
fn flat(reference: Date, rate: f64, dc: DayCounter) -> Handle<dyn YieldTermStructure> {
    Handle::new(shared(FlatForward::with_rate(reference, rate, dc, Compounding::Continuous,
        Frequency::Annual)) as Shared<dyn YieldTermStructure>)
}

/// Scalar market inputs followed by native Heston parameters.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HestonProcessConfig {
    pub risk_free_rate: f64, pub dividend_yield: f64, pub spot: f64,
    pub v0: f64, pub kappa: f64, pub theta: f64, pub sigma: f64, pub rho: f64,
    pub reference_date: i32, pub day_counter: u64,
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_heston_process_new(ctx: *mut Context,
    config: HestonProcessConfig, out: *mut u64, error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx, error, |c| {
        check_ptr(out)?;
        let d = date(config.reference_date)?;
        let dc = day_counter(c, config.day_counter)?;
        let process = HestonProcess::new(flat(d, config.risk_free_rate, dc.clone()),
            flat(d, config.dividend_yield, dc),
            Handle::new(shared(SimpleQuote::new(config.spot)) as Shared<dyn Quote>),
            config.v0, config.kappa, config.theta, config.sigma, config.rho);
        output(out, c.insert(shared(process))?)
    }) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_heston_model_new(ctx: *mut Context, process: u64,
    out: *mut u64, error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx, error, |c| {
        check_ptr(out)?;
        let model = HestonModel::new(c.get::<Shared<HestonProcess>>(process)?)?;
        output(out, c.insert(model)?)
    }) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_hullwhite_new(ctx: *mut Context, curve: u64,
    a: f64, sigma: f64, out: *mut u64, error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx, error, |c| {
        check_ptr(out)?;
        let model = HullWhite::new(c.get::<Handle<dyn YieldTermStructure>>(curve)?, a, sigma)?;
        output(out, c.insert(model)?)
    }) }
}

/// Kind 0 Heston process, 1 Heston model, 2 HullWhite.
/// Heston fields: v0, kappa, theta, sigma, rho. HullWhite fields: a, sigma, r0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_model_parameter(ctx: *mut Context, handle: u64,
    kind: i32, field: usize, out: *mut f64, error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx, error, |c| {
        check_ptr(out)?;
        let values = match kind {
            0 => { let p = c.get::<Shared<HestonProcess>>(handle)?;
                vec![p.v0(), p.kappa(), p.theta(), p.sigma(), p.rho()] },
            1 => { let p = c.get::<SharedMut<HestonModel>>(handle)?; let p = p.borrow();
                vec![p.v0(), p.kappa(), p.theta(), p.sigma(), p.rho()] },
            2 => { let p = c.get::<SharedMut<HullWhite>>(handle)?; let p = p.borrow();
                let v = p.calibrated_model().params(); vec![v[0], v[1], p.r0()] },
            _ => return Err(BindingError::invalid("unknown model kind")),
        };
        output(out, *values.get(field).ok_or_else(|| BindingError::invalid("unknown model field"))?)
    }) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_hullwhite_bond_option(ctx: *mut Context, model: u64,
    kind: i32, strike: f64, maturity: f64, bond_maturity: f64, out: *mut f64,
    error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx, error, |c| {
        check_ptr(out)?;
        let model = c.get::<SharedMut<HullWhite>>(model)?;
        let value = model.borrow().discount_bond_option(crate::options_api::option_type(kind)?,
            strike, maturity, bond_maturity)?;
        output(out, value)
    }) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_levenberg_marquardt_new(ctx: *mut Context,
    epsfcn: f64, xtol: f64, gtol: f64, jacobian: i32, out: *mut u64,
    error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx, error, |c| {
        check_ptr(out)?;
        if !(0..=1).contains(&jacobian) { return Err(BindingError::invalid("jacobian must be 0 or 1")); }
        output(out, c.insert(shared_mut(LevenbergMarquardt::new(epsfcn, xtol, gtol, jacobian != 0)))?)
    }) }
}
/// Presence flags distinguish omitted criteria from explicit zero values.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct EndCriteriaConfig {
    pub max_iterations: usize, pub stationary_iterations: usize,
    pub root_epsilon: f64, pub function_epsilon: f64, pub gradient_epsilon: f64,
    pub has_stationary: i32, pub has_gradient: i32,
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_end_criteria_new(ctx: *mut Context, config: EndCriteriaConfig,
    out: *mut u64, error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx, error, |c| {
        check_ptr(out)?;
        if !(0..=1).contains(&config.has_stationary) || !(0..=1).contains(&config.has_gradient) {
            return Err(BindingError::invalid("presence flags must be 0 or 1"));
        }
        let end = EndCriteria::new(config.max_iterations,
            (config.has_stationary != 0).then_some(config.stationary_iterations),
            config.root_epsilon, config.function_epsilon,
            (config.has_gradient != 0).then_some(config.gradient_epsilon))?;
        output(out, c.insert(end)?)
    }) }
}
