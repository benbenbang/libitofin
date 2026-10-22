//! Immutable Black FX delta and ATM convention adapters.
use crate::boundary::*;
use crate::options_api::option_type;
use libitofin::pricingengines::BlackDeltaCalculator;
use libitofin::quotes::{AtmType, DeltaType};

/// Spot delta convention.
pub const ITOFIN_DELTA_SPOT: i32 = 0;
/// Forward delta convention.
pub const ITOFIN_DELTA_FWD: i32 = 1;
/// Premium-adjusted spot delta convention.
pub const ITOFIN_DELTA_PA_SPOT: i32 = 2;
/// Premium-adjusted forward delta convention.
pub const ITOFIN_DELTA_PA_FWD: i32 = 3;
/// Unspecified ATM convention, rejected by atm_strike.
pub const ITOFIN_ATM_NULL: i32 = 0;
/// Spot ATM strike.
pub const ITOFIN_ATM_SPOT: i32 = 1;
/// Forward ATM strike.
pub const ITOFIN_ATM_FWD: i32 = 2;
/// Delta-neutral ATM strike.
pub const ITOFIN_ATM_DELTA_NEUTRAL: i32 = 3;
/// Maximum-vega ATM strike.
pub const ITOFIN_ATM_VEGA_MAX: i32 = 4;
/// Maximum-gamma ATM strike.
pub const ITOFIN_ATM_GAMMA_MAX: i32 = 5;
/// Fifty-delta ATM strike, valid only for unadjusted forward delta.
pub const ITOFIN_ATM_PUT_CALL_50: i32 = 6;

fn delta_type(value: i32) -> BindingResult<DeltaType> {
    match value {
        ITOFIN_DELTA_SPOT => Ok(DeltaType::Spot),
        ITOFIN_DELTA_FWD => Ok(DeltaType::Fwd),
        ITOFIN_DELTA_PA_SPOT => Ok(DeltaType::PaSpot),
        ITOFIN_DELTA_PA_FWD => Ok(DeltaType::PaFwd),
        _ => Err(BindingError::invalid("unknown delta convention")),
    }
}

fn atm_type(value: i32) -> BindingResult<AtmType> {
    match value {
        ITOFIN_ATM_NULL => Ok(AtmType::Null),
        ITOFIN_ATM_SPOT => Ok(AtmType::Spot),
        ITOFIN_ATM_FWD => Ok(AtmType::Fwd),
        ITOFIN_ATM_DELTA_NEUTRAL => Ok(AtmType::DeltaNeutral),
        ITOFIN_ATM_VEGA_MAX => Ok(AtmType::VegaMax),
        ITOFIN_ATM_GAMMA_MAX => Ok(AtmType::GammaMax),
        ITOFIN_ATM_PUT_CALL_50 => Ok(AtmType::PutCall50),
        _ => Err(BindingError::invalid("unknown ATM convention")),
    }
}

/// Copy validated parameters into an immutable calculator. Option kind is
/// 0 call or 1 put, preserving the existing ABI mapping. Deviation is volatility
/// times square root of time, not volatility or maturity separately.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_delta_new(
    ctx: *mut Context,
    option_kind: i32,
    delta_kind: i32,
    spot: f64,
    domestic_discount: f64,
    foreign_discount: f64,
    std_dev: f64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let calc = BlackDeltaCalculator::new(
                option_type(option_kind)?,
                delta_type(delta_kind)?,
                spot,
                domestic_discount,
                foreign_discount,
                std_dev,
            )?;
            output(out, c.insert(calc)?)
        })
    }
}

unsafe fn calculate(
    ctx: *mut Context,
    calculator: u64,
    out: *mut f64,
    error: *mut ItofinError,
    f: impl FnOnce(BlackDeltaCalculator) -> BindingResult<f64>,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let value = f(c.get::<BlackDeltaCalculator>(calculator)?)?;
            if !value.is_finite() {
                return Err(BindingError::invalid("Black delta result must be finite"));
            }
            output(out, value)
        })
    }
}

/// Signed delta at a finite nonnegative strike. Zero deviation is supported;
/// premium-adjusted put deltas are not clamped to minus one.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_delta_delta_from_strike(
    ctx: *mut Context,
    calculator: u64,
    strike: f64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        calculate(ctx, calculator, out, error, |c| {
            Ok(c.delta_from_strike(strike)?)
        })
    }
}

/// Invert a signed interior delta with positive deviation. Premium-adjusted
/// calls select the high-strike root, not necessarily the original strike.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_delta_strike_from_delta(
    ctx: *mut Context,
    calculator: u64,
    delta: f64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        calculate(ctx, calculator, out, error, |c| {
            Ok(c.strike_from_delta(delta)?)
        })
    }
}

/// ATM strike for a stable ITOFIN_ATM_* value. Null is an error; PutCall50
/// requires ITOFIN_DELTA_FWD, not premium-adjusted forward delta.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_delta_atm_strike(
    ctx: *mut Context,
    calculator: u64,
    atm_kind: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        calculate(ctx, calculator, out, error, |c| {
            Ok(c.atm_strike(atm_type(atm_kind)?)?)
        })
    }
}

#[cfg(test)]
#[path = "black_delta_api_tests.rs"]
mod tests;
