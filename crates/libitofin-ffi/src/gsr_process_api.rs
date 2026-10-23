//! Constant-grid forward-measure GSR process adapters.
use crate::boundary::*;
use libitofin::processes::forwardmeasureprocess::ForwardMeasureProcess1D;
use libitofin::processes::gsrprocess::GsrProcess;
use libitofin::shared::{SharedMut, shared_mut};
use libitofin::stochasticprocess::StochasticProcess1D;

/// Copies a constant parameter grid. Signed reversion and zero volatility are valid.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_gsr_process_new(
    ctx: *mut Context,
    times: *const f64,
    times_len: usize,
    vols: *const f64,
    vols_len: usize,
    reversions: *const f64,
    reversions_len: usize,
    horizon: f64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let p = GsrProcess::new(
                input_slice(times, times_len)?.to_vec(),
                input_slice(vols, vols_len)?.to_vec(),
                input_slice(reversions, reversions_len)?.to_vec(),
                horizon,
            )?;
            output(out, c.insert(shared_mut(p))?)
        })
    }
}

/// Query kind: 0 x0, 1 horizon, 2 sigma(t), 3 reversion(t), 4 y(t),
/// 5 G(t,end), 6 drift(t,x), 7 diffusion(t,x), 8 expectation(t,x,dt),
/// 9 standard deviation(t,x,dt), 10 variance(t,x,dt), 11 evolve(t,x,dt,dw).
/// Kind 5 uses `dt` as the absolute end time. Other unused inputs are ignored.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_gsr_process_query(
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
            let process = c.get::<SharedMut<GsrProcess>>(process)?;
            let p = process.borrow();
            let value = match kind {
                0 => p.x0()?,
                1 => p.forward_measure_time(),
                2 => p.sigma(t)?,
                3 => p.reversion(t)?,
                4 => p.y(t)?,
                5 => p.g(t, dt)?,
                6 => p.drift(t, x)?,
                7 => p.diffusion(t, x)?,
                8 => p.expectation(t, x, dt)?,
                9 => p.std_deviation(t, x, dt)?,
                10 => p.variance(t, x, dt)?,
                11 => p.evolve(t, x, dt, dw)?,
                _ => return Err(BindingError::invalid("unknown GSR query kind")),
            };
            output(out, value)
        })
    }
}

/// Updates the horizon through the native notifying setter. Invalid values preserve it.
/// # Safety
/// Follow the crate-level C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_gsr_process_set_forward_measure_time(
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
            c.get::<SharedMut<GsrProcess>>(process)?
                .borrow_mut()
                .set_forward_measure_time(horizon)?;
            Ok(())
        })
    }
}

#[cfg(test)]
#[path = "gsr_process_api_tests.rs"]
mod tests;
