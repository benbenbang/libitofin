//! SABR smile sections use the same core formulas and admissibility checks as Python.
use crate::boundary::*;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::volatility::{SabrSmileSection, SmileSection, VolatilityType};
pub(crate) fn volatility_type(kind: i32) -> BindingResult<VolatilityType> {
    match kind {
        0 => Ok(VolatilityType::ShiftedLognormal),
        1 => Ok(VolatilityType::Normal),
        _ => Err(BindingError::invalid("unknown volatility type")),
    }
}
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_sabr_smile_new(
    ctx: *mut Context,
    exercise_time: f64,
    forward: f64,
    alpha: f64,
    beta: f64,
    nu: f64,
    rho: f64,
    shift: f64,
    kind: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if [exercise_time, forward, alpha, beta, nu, rho, shift]
                .iter()
                .any(|v| !v.is_finite())
            {
                return Err(BindingError::invalid("SABR inputs must be finite"));
            }
            let smile = SabrSmileSection::with_exercise_time(
                exercise_time,
                forward,
                alpha,
                beta,
                nu,
                rho,
                shift,
                volatility_type(kind)?,
            )?;
            output(out, c.insert(shared(smile))?)
        })
    }
}
/// Query: 0 volatility, 1 variance, 2 exercise time, 3 ATM, 4 alpha, 5 beta, 6 nu, 7 rho.
/// # Safety
/// Follow the crate C caller contract; arrays must have their stated lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_sabr_smile_query(
    ctx: *mut Context,
    id: u64,
    kind: i32,
    strike: f64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if !strike.is_finite() {
                return Err(BindingError::invalid("strike must be finite"));
            }
            let v = c.get::<Shared<SabrSmileSection>>(id)?;
            output(
                out,
                match kind {
                    0 => v.volatility(strike)?,
                    1 => v.variance(strike)?,
                    2 => v.exercise_time(),
                    3 => v
                        .atm_level()
                        .ok_or_else(|| BindingError::invalid("ATM level is absent"))?,
                    4 => v.alpha(),
                    5 => v.beta(),
                    6 => v.nu(),
                    7 => v.rho(),
                    _ => return Err(BindingError::invalid("unknown SABR query")),
                },
            )
        })
    }
}
