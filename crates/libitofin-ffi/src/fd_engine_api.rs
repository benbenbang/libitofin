//! Finite-difference Black-Scholes vanilla engine bindings.

use crate::boundary::*;
use crate::time_api::date;
use libitofin::cashflows::{Dividend, dividend_vector};
use libitofin::methods::finitedifferences::solvers::FdmSchemeDesc;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::vanilla::{
    AnalyticDividendEuropeanEngine, FdBlackScholesVanillaEngine,
};
use libitofin::processes::GeneralizedBlackScholesProcess;
use libitofin::shared::{Shared, SharedMut, shared_mut};

/// Optional fields: time grid=1, equity grid=2, damping steps=4, scheme=8.
/// Scheme 0 is Douglas and scheme 1 is implicit Euler.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ItofinFdConfig {
    pub present: u32,
    pub t_grid: u64,
    pub x_grid: u64,
    pub damping_steps: u64,
    pub scheme: i32,
}

fn fd_config(cfg: ItofinFdConfig) -> BindingResult<(usize, usize, usize, FdmSchemeDesc)> {
    if cfg.present & !15 != 0 {
        return Err(BindingError::invalid(
            "invalid FD configuration presence mask",
        ));
    }
    let t_grid = if cfg.present & 1 != 0 {
        cfg.t_grid
    } else {
        100
    };
    let x_grid = if cfg.present & 2 != 0 {
        cfg.x_grid
    } else {
        100
    };
    let damping_steps = if cfg.present & 4 != 0 {
        cfg.damping_steps
    } else {
        0
    };
    let scheme = if cfg.present & 8 != 0 { cfg.scheme } else { 0 };
    let scheme = match scheme {
        0 => FdmSchemeDesc::douglas(),
        1 => FdmSchemeDesc::implicit_euler(),
        _ => return Err(BindingError::invalid("unsupported FD scheme")),
    };
    if t_grid == 0 || x_grid < 3 || t_grid.checked_add(damping_steps).is_none() {
        return Err(BindingError::invalid("invalid FD grid or damping steps"));
    }
    let t_grid = usize::try_from(t_grid)
        .map_err(|_| BindingError::invalid("FD time grid exceeds platform size"))?;
    let x_grid = usize::try_from(x_grid)
        .map_err(|_| BindingError::invalid("FD equity grid exceeds platform size"))?;
    let damping_steps = usize::try_from(damping_steps)
        .map_err(|_| BindingError::invalid("FD damping steps exceed platform size"))?;
    if t_grid.checked_add(damping_steps).is_none() {
        return Err(BindingError::invalid("FD step count exceeds platform size"));
    }
    Ok((t_grid, x_grid, damping_steps, scheme))
}

/// # Safety
/// `dates` and `amounts` must be valid for `n` elements; both may be null
/// when `n` is zero.
unsafe fn cash_dividends(
    dates: *const i32,
    amounts: *const f64,
    n: usize,
) -> BindingResult<Vec<Shared<dyn Dividend>>> {
    let dates = unsafe { input_slice(dates, n) }?
        .iter()
        .copied()
        .map(date)
        .collect::<BindingResult<Vec<_>>>()?;
    let amounts = unsafe { input_slice(amounts, n) }?;
    Ok(dividend_vector(&dates, amounts)?)
}

/// Construct an FD engine retaining the Black-Scholes process. Attach it to a
/// vanilla option with `itofin_option_set_engine` kind 2.
/// # Safety
/// Outputs must be aligned, live and non-overlapping. Context and handles must
/// belong to the calling thread; serialize calls including destruction.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_fd_black_scholes_engine_new(
    ctx: *mut Context,
    process: u64,
    cfg: ItofinFdConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let (t_grid, x_grid, damping_steps, scheme) = fd_config(cfg)?;
            let process = c.get::<Shared<GeneralizedBlackScholesProcess>>(process)?;
            let engine = shared_mut(FdBlackScholesVanillaEngine::new(
                process,
                t_grid,
                x_grid,
                damping_steps,
                scheme,
            )) as SharedMut<dyn PricingEngine>;
            output(out, c.insert(engine)?)
        })
    }
}

/// Construct an FD engine paying `n` cash dividends, one fixed `amounts[i]` on
/// the serial date `dates[i]`, under the Spot model. Dividends outside the
/// option life are dropped. Both arrays share the single length `n`; with
/// `n` zero they may be null and the engine equals the plain constructor.
/// The Escrowed cash-dividend model is not offered (deferred to #828).
/// Attach with `itofin_option_set_engine` kind 2. A dividend dated on the
/// evaluation date leaves theta unavailable: querying it is a core error.
/// # Safety
/// Follow the crate C caller contract; arrays must have length `n`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_fd_black_scholes_engine_with_cash_dividends(
    ctx: *mut Context,
    process: u64,
    cfg: ItofinFdConfig,
    dates: *const i32,
    amounts: *const f64,
    n: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let (t_grid, x_grid, damping_steps, scheme) = fd_config(cfg)?;
            let dividends = cash_dividends(dates, amounts, n)?;
            let process = c.get::<Shared<GeneralizedBlackScholesProcess>>(process)?;
            let engine = shared_mut(FdBlackScholesVanillaEngine::with_cash_dividends(
                process,
                dividends,
                t_grid,
                x_grid,
                damping_steps,
                scheme,
            )) as SharedMut<dyn PricingEngine>;
            output(out, c.insert(engine)?)
        })
    }
}

/// Construct the analytic European engine paying `n` fixed cash dividends,
/// `amounts[i]` on the serial date `dates[i]`, retaining the process. Arrays
/// may be null when `n` is zero. Attach with `itofin_option_set_engine` kind
/// 2; pricing a non-European exercise is a core error.
/// # Safety
/// Follow the crate C caller contract; arrays must have length `n`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_analytic_dividend_european_engine_new(
    ctx: *mut Context,
    process: u64,
    dates: *const i32,
    amounts: *const f64,
    n: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let dividends = cash_dividends(dates, amounts, n)?;
            let process = c.get::<Shared<GeneralizedBlackScholesProcess>>(process)?;
            let engine = shared_mut(AnalyticDividendEuropeanEngine::new(process, dividends))
                as SharedMut<dyn PricingEngine>;
            output(out, c.insert(engine)?)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::null_mut;

    #[test]
    fn invalid_configuration_preserves_output_and_context() {
        let mut context = Context::new();
        let mut out = 0;
        let mut error = ItofinError {
            code: 0,
            message: [0; 1024],
        };
        unsafe {
            assert_ne!(
                itofin_fd_black_scholes_engine_new(
                    &mut context,
                    0,
                    ItofinFdConfig {
                        present: 16,
                        ..ItofinFdConfig::default()
                    },
                    &mut out,
                    &mut error,
                ),
                0
            );
            assert_eq!(out, 0);
            assert_ne!(
                itofin_fd_black_scholes_engine_new(
                    &mut context,
                    0,
                    ItofinFdConfig {
                        present: 8,
                        scheme: 2,
                        ..ItofinFdConfig::default()
                    },
                    &mut out,
                    &mut error,
                ),
                0
            );
            assert_eq!(out, 0);
            assert_ne!(
                itofin_fd_black_scholes_engine_new(
                    &mut context,
                    0,
                    ItofinFdConfig::default(),
                    null_mut(),
                    &mut error,
                ),
                0
            );
            assert_eq!(out, 0);
            assert_eq!(
                crate::market_api::itofin_quote_new(&mut context, 80.0, &mut out, &mut error),
                0
            );
            assert_ne!(out, 0);
        }
    }
}
