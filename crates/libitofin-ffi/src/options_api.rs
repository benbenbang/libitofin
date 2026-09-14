//! Vanilla option adapters. All valuation and lazy invalidation stay in the core.
use crate::boundary::*;
use crate::time_api::date;
use libitofin::exercise::{AmericanExercise, EuropeanExercise, Exercise};
use libitofin::instrument::Instrument;
use libitofin::instruments::{PlainVanillaPayoff, StrikedTypePayoff, VanillaOption};
use libitofin::models::HestonModel;
use libitofin::option::OptionType;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::AnalyticEuropeanEngine;
use libitofin::pricingengines::vanilla::analytichestonengine::AnalyticHestonEngine;
use libitofin::processes::GeneralizedBlackScholesProcess;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::time::date::Date;

pub(crate) fn option_type(value: i32) -> BindingResult<OptionType> {
    match value { 0 => Ok(OptionType::Call), 1 => Ok(OptionType::Put),
        _ => Err(BindingError::invalid("unknown option type")) }
}

/// `american`: 0 European, 1 American; earliest ignored for European exercise.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_option_new(ctx: *mut Context, kind: i32, strike: f64,
    earliest: i32, expiry: i32, american: i32, settings: u64, out: *mut u64,
    error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx, error, |c| {
        check_ptr(out)?;
        let payoff = shared(PlainVanillaPayoff::new(option_type(kind)?, strike)) as Shared<dyn StrikedTypePayoff>;
        let exercise: Shared<dyn Exercise> = match american {
            0 => shared(EuropeanExercise::new(date(expiry)?)),
            1 => shared(AmericanExercise::over(date(earliest)?, date(expiry)?)?),
            _ => return Err(BindingError::invalid("american must be 0 or 1")),
        };
        let option = VanillaOption::new(payoff, exercise, c.get::<Shared<Settings<Date>>>(settings)?);
        output(out, c.insert(shared_mut(option))?)
    }) }
}

/// Engine kind: 0 analytic European (BSM process), 1 analytic Heston (model), 2 MC engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_option_set_engine(ctx: *mut Context, option: u64,
    source: u64, kind: i32, integration_order: usize, error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx, error, |c| {
        let option = c.get::<SharedMut<VanillaOption>>(option)?;
        let engine: SharedMut<dyn PricingEngine> = match kind {
            0 => shared_mut(AnalyticEuropeanEngine::new(c.get::<Shared<GeneralizedBlackScholesProcess>>(source)?)),
            1 => shared_mut(AnalyticHestonEngine::new(c.get::<SharedMut<HestonModel>>(source)?, integration_order)?),
            2 => c.get(source)?,
            _ => return Err(BindingError::invalid("unknown option engine kind")),
        };
        option.borrow_mut().base_mut().set_pricing_engine(engine);
        Ok(())
    }) }
}

/// Field: 0 NPV, 1 delta, 2 gamma, 3 theta, 4 vega, 5 rho, 6 dividend rho,
/// 7 error estimate, 8 exercise probability. Missing fields return core errors.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_option_value(ctx: *mut Context, option: u64,
    field: i32, out: *mut f64, error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx, error, |c| {
        check_ptr(out)?;
        let option = c.get::<SharedMut<VanillaOption>>(option)?;
        let mut option = option.borrow_mut();
        let value = match field {
            0 => option.npv()?, 1 => option.delta()?, 2 => option.gamma()?,
            3 => option.theta()?, 4 => option.vega()?, 5 => option.rho()?,
            6 => option.dividend_rho()?, 7 => option.error_estimate()?,
            8 => option.result::<f64>("exerciseProbability")?,
            _ => return Err(BindingError::invalid("unknown option result field")),
        };
        output(out, value)
    }) }
}

/// Calculate if requested, then report cache validity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_option_calculate(ctx: *mut Context, option: u64,
    calculate: i32, out: *mut i32, error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx, error, |c| {
        check_ptr(out)?;
        let option = c.get::<SharedMut<VanillaOption>>(option)?;
        let mut option = option.borrow_mut();
        match calculate { 0 => (), 1 => option.calculate()?,
            _ => return Err(BindingError::invalid("calculate must be 0 or 1")) }
        output(out, i32::from(option.base().is_calculated()))
    }) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_option_results(ctx: *mut Context, option: u64,
    out: *mut u64, error: *mut ItofinError) -> i32 {
    unsafe { with_context(ctx, error, |c| {
        check_ptr(out)?;
        let option = c.get::<SharedMut<VanillaOption>>(option)?;
        let mut option = option.borrow_mut();
        option.calculate()?;
        output(out, c.insert(crate::results_api::snapshot(option.base()))?)
    }) }
}
