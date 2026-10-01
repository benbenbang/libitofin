//! Live Merton76 processes and European jump-diffusion pricing engines.
use crate::boundary::*;
use crate::market_api::quote;
use crate::time_api::date;
use libitofin::handle::Handle;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::JumpDiffusionEngine;
use libitofin::processes::Merton76Process;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::volatility::BlackVolTermStructure;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;

/// Retain live spot, market curves, volatility and three jump quotes.
/// # Safety
/// Pointers must be aligned, live and non-overlapping. Context and handles
/// belong to the calling thread; serialize calls including destruction.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_merton76_new(
    ctx: *mut Context,
    spot: u64,
    risk_free: u64,
    dividend: u64,
    volatility: u64,
    jump_intensity: u64,
    log_mean_jump: u64,
    log_jump_volatility: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let process = Merton76Process::new(
                quote(c, spot)?,
                c.get::<Handle<dyn YieldTermStructure>>(dividend)?,
                c.get::<Handle<dyn YieldTermStructure>>(risk_free)?,
                c.get::<Handle<dyn BlackVolTermStructure>>(volatility)?,
                quote(c, jump_intensity)?,
                quote(c, log_mean_jump)?,
                quote(c, log_jump_volatility)?,
            )?;
            output(out, c.insert(shared(process))?)
        })
    }
}

/// Field: 0 spot, 1 jump intensity, 2 log mean jump, 3 log jump volatility.
/// # Safety
/// Pointers must be aligned, live and non-overlapping. Context and handles
/// belong to the calling thread; serialize calls including destruction.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_merton76_value(
    ctx: *mut Context,
    process: u64,
    field: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let process = c.get::<Shared<Merton76Process>>(process)?;
            let value = match field {
                0 => process.x0()?,
                1 => process.jump_intensity().current_link()?.value()?,
                2 => process.log_mean_jump().current_link()?.value()?,
                3 => process.log_jump_volatility().current_link()?.value()?,
                _ => return Err(BindingError::invalid("unknown Merton76 process field")),
            };
            output(out, value)
        })
    }
}

/// Convert a date using the retained market's risk-free day counter.
/// # Safety
/// Pointers must be aligned, live and non-overlapping. Context and handles
/// belong to the calling thread; serialize calls including destruction.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_merton76_time(
    ctx: *mut Context,
    process: u64,
    date_serial: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let process = c.get::<Shared<Merton76Process>>(process)?;
            output(out, process.time(&date(date_serial)?)?)
        })
    }
}

/// Retain a Merton76 process for European plain-vanilla option engine kind 2.
/// Conventional settings are relative_accuracy=1e-4, max_iterations=100.
/// # Safety
/// Pointers must be aligned, live and non-overlapping. Context and handles
/// belong to the calling thread; serialize calls including destruction.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_jump_diffusion_engine_new(
    ctx: *mut Context,
    process: u64,
    relative_accuracy: f64,
    max_iterations: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let engine: SharedMut<dyn PricingEngine> = shared_mut(JumpDiffusionEngine::new(
                c.get::<Shared<Merton76Process>>(process)?,
                relative_accuracy,
                max_iterations,
            )?);
            output(out, c.insert(engine)?)
        })
    }
}
