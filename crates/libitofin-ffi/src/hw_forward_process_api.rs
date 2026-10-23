//! Hull-White forward-measure process adapters retaining the supplied live curve.
use crate::boundary::*;
use crate::rates_api::curve;
use libitofin::processes::{ForwardMeasureProcess1D, HullWhiteForwardProcess};
use libitofin::shared::{SharedMut, shared_mut};
use libitofin::stochasticprocess::StochasticProcess1D;

/// Retains the exact curve handle. Initial short rate is captured; horizon starts at zero.
/// # Safety
/// Follow the crate-level owning-thread and pointer contract. Output on success only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_hw_forward_process_new(
    ctx: *mut Context,
    yield_curve: u64,
    a: f64,
    sigma: f64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let process = HullWhiteForwardProcess::new(curve(c, yield_curve)?, a, sigma)?;
            output(out, c.insert(shared_mut(process))?)
        })
    }
}

/// Thirteen query kinds: 0 x0, 1 a, 2 sigma, 3 horizon, 4 alpha(t), 5 B(t,dt),
/// 6 M_T(t,x,dt), 7 drift(t,x), 8 diffusion(t,x), 9 expectation(t,x,dt),
/// 10 std deviation(t,x,dt), 11 variance(t,x,dt), 12 evolve(t,x,dt,dw).
/// For kinds 5/6, dt is an absolute maturity; x is the second endpoint for kind 6.
/// Unused inputs are ignored. Times may exceed the forward-measure horizon.
/// # Safety
/// Follow the crate-level owning-thread and pointer contract. Output on success only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_hw_forward_process_query(
    ctx: *mut Context,
    process: u64,
    kind: i32,
    t: f64,
    x: f64,
    dt: f64,
    dw: f64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let process = c.get::<SharedMut<HullWhiteForwardProcess>>(process)?;
            let p = process.borrow();
            let value = match kind {
                0 => p.x0()?,
                1 => p.a(),
                2 => p.sigma(),
                3 => p.forward_measure_time(),
                4 => p.alpha(t)?,
                5 => p.b(t, dt)?,
                6 => p.m_t(t, x, dt)?,
                7 => p.drift(t, x)?,
                8 => p.diffusion(t, x)?,
                9 => p.expectation(t, x, dt)?,
                10 => p.std_deviation(t, x, dt)?,
                11 => p.variance(t, x, dt)?,
                12 => p.evolve(t, x, dt, dw)?,
                _ => {
                    return Err(BindingError::invalid(
                        "unknown Hull-White forward query kind",
                    ));
                }
            };
            output(out, value)
        })
    }
}

/// Replaces the horizon using the native notifying setter. Invalid values preserve state.
/// # Safety
/// Follow the crate-level owning-thread and pointer contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_hw_forward_process_set_forward_measure_time(
    ctx: *mut Context,
    process: u64,
    horizon: f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            c.get::<SharedMut<HullWhiteForwardProcess>>(process)?
                .borrow_mut()
                .set_forward_measure_time(horizon)?;
            Ok(())
        })
    }
}

#[cfg(test)]
#[path = "hw_forward_process_api_tests.rs"]
mod tests;
