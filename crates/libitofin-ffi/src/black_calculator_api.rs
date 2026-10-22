//! Immutable plain-vanilla Black 1976 scalar calculator adapters.
use crate::boundary::*;
use crate::options_api::option_type;
use libitofin::pricingengines::BlackCalculator;

/// Copy finite parameters for a plain-vanilla call (0) or put (1).
/// Strike and std_dev are nonnegative; forward and discount are positive.
/// std_dev is volatility times sqrt(time). No payoff handle is accepted.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_calculator_new(
    ctx: *mut Context,
    option_kind: i32,
    strike: f64,
    forward: f64,
    std_dev: f64,
    discount: f64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let calculator = BlackCalculator::new(
                option_type(option_kind)?,
                strike,
                forward,
                std_dev,
                discount,
            )?;
            output(out, c.insert(calculator)?)
        })
    }
}

unsafe fn calculate(
    ctx: *mut Context,
    calculator: u64,
    out: *mut f64,
    error: *mut ItofinError,
    f: impl FnOnce(BlackCalculator) -> BindingResult<f64>,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let value = f(c.get::<BlackCalculator>(calculator)?)?;
            if !value.is_finite() {
                return Err(BindingError::invalid(
                    "BlackCalculator result must be finite",
                ));
            }
            output(out, value)
        })
    }
}

/// Present value.
/// Nonfinite native results are errors; finite native edge behavior is unchanged.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_calculator_value(
    ctx: *mut Context,
    calculator: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe { calculate(ctx, calculator, out, error, |c| Ok(c.value())) }
}

/// Forward delta.
/// Nonfinite native results are errors; finite native edge behavior is unchanged.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_calculator_delta_forward(
    ctx: *mut Context,
    calculator: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe { calculate(ctx, calculator, out, error, |c| Ok(c.delta_forward())) }
}

/// Spot delta, requiring positive finite spot.
/// Nonfinite native results are errors; finite native edge behavior is unchanged.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_calculator_delta(
    ctx: *mut Context,
    calculator: u64,
    spot: f64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe { calculate(ctx, calculator, out, error, |c| Ok(c.delta(spot)?)) }
}

/// Forward elasticity; native finite MAX/MIN sentinels are retained.
/// Nonfinite native results are errors; finite native edge behavior is unchanged.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_calculator_elasticity_forward(
    ctx: *mut Context,
    calculator: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe { calculate(ctx, calculator, out, error, |c| Ok(c.elasticity_forward())) }
}

/// Spot elasticity, requiring positive finite spot.
/// Nonfinite native results are errors; finite native edge behavior is unchanged.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_calculator_elasticity(
    ctx: *mut Context,
    calculator: u64,
    spot: f64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe { calculate(ctx, calculator, out, error, |c| Ok(c.elasticity(spot)?)) }
}

/// Forward gamma.
/// Nonfinite native results are errors; finite native edge behavior is unchanged.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_calculator_gamma_forward(
    ctx: *mut Context,
    calculator: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe { calculate(ctx, calculator, out, error, |c| Ok(c.gamma_forward())) }
}

/// Spot gamma, requiring positive finite spot.
/// Nonfinite native results are errors; finite native edge behavior is unchanged.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_calculator_gamma(
    ctx: *mut Context,
    calculator: u64,
    spot: f64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe { calculate(ctx, calculator, out, error, |c| Ok(c.gamma(spot)?)) }
}

/// Theta per year. Native expiry shortcut is retained.
/// Nonfinite native results are errors; finite native edge behavior is unchanged.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_calculator_theta(
    ctx: *mut Context,
    calculator: u64,
    spot: f64,
    maturity: f64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        calculate(
            ctx,
            calculator,
            out,
            error,
            |c| Ok(c.theta(spot, maturity)?),
        )
    }
}

/// Theta divided by 365. Native expiry shortcut is retained.
/// Nonfinite native results are errors; finite native edge behavior is unchanged.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_calculator_theta_per_day(
    ctx: *mut Context,
    calculator: u64,
    spot: f64,
    maturity: f64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        calculate(ctx, calculator, out, error, |c| {
            Ok(c.theta_per_day(spot, maturity)?)
        })
    }
}

/// Vega for finite nonnegative maturity.
/// Nonfinite native results are errors; finite native edge behavior is unchanged.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_calculator_vega(
    ctx: *mut Context,
    calculator: u64,
    maturity: f64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe { calculate(ctx, calculator, out, error, |c| Ok(c.vega(maturity)?)) }
}

/// Discount-rate rho for finite nonnegative maturity.
/// Nonfinite native results are errors; finite native edge behavior is unchanged.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_calculator_rho(
    ctx: *mut Context,
    calculator: u64,
    maturity: f64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe { calculate(ctx, calculator, out, error, |c| Ok(c.rho(maturity)?)) }
}

/// Growth-rate rho for finite nonnegative maturity.
/// Nonfinite native results are errors; finite native edge behavior is unchanged.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_calculator_dividend_rho(
    ctx: *mut Context,
    calculator: u64,
    maturity: f64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        calculate(ctx, calculator, out, error, |c| {
            Ok(c.dividend_rho(maturity)?)
        })
    }
}

/// Native N(d2), independent of option sign.
/// Nonfinite native results are errors; finite native edge behavior is unchanged.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_calculator_itm_cash_probability(
    ctx: *mut Context,
    calculator: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        calculate(
            ctx,
            calculator,
            out,
            error,
            |c| Ok(c.itm_cash_probability()),
        )
    }
}

/// Native N(d1), independent of option sign.
/// Nonfinite native results are errors; finite native edge behavior is unchanged.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_calculator_itm_asset_probability(
    ctx: *mut Context,
    calculator: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        calculate(ctx, calculator, out, error, |c| {
            Ok(c.itm_asset_probability())
        })
    }
}

/// Strike sensitivity.
/// Nonfinite native results are errors; finite native edge behavior is unchanged.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_calculator_strike_sensitivity(
    ctx: *mut Context,
    calculator: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe { calculate(ctx, calculator, out, error, |c| Ok(c.strike_sensitivity())) }
}

/// Forward coefficient in the native value formula.
/// Nonfinite native results are errors; finite native edge behavior is unchanged.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_calculator_alpha(
    ctx: *mut Context,
    calculator: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe { calculate(ctx, calculator, out, error, |c| Ok(c.alpha())) }
}

/// Strike coefficient in the native value formula.
/// Nonfinite native results are errors; finite native edge behavior is unchanged.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_black_calculator_beta(
    ctx: *mut Context,
    calculator: u64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe { calculate(ctx, calculator, out, error, |c| Ok(c.beta())) }
}

#[cfg(test)]
#[path = "black_calculator_api_tests.rs"]
mod tests;
