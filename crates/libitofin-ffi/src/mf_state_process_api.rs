//! Immutable standalone Markov-functional state process adapters.
use crate::boundary::*;
use libitofin::processes::MfStateProcess;
use libitofin::shared::{Shared, shared};
use libitofin::stochasticprocess::StochasticProcess1D;

/// Copies the parameter arrays. Volatility count must exceed time count by one.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_mf_state_process_new(
    ctx: *mut Context,
    reversion: f64,
    times: *const f64,
    times_len: usize,
    vols: *const f64,
    vols_len: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let p = MfStateProcess::new(
                reversion,
                input_slice(times, times_len)?.to_vec().into(),
                input_slice(vols, vols_len)?.to_vec().into(),
            )?;
            output(out, c.insert(shared(p))?)
        })
    }
}

/// Query kind: 0 reversion, 1 x0, 2 drift, 3 diffusion, 4 expectation,
/// 5 standard deviation, 6 variance, 7 evolve. Unused inputs are ignored.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_mf_state_process_query(
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
            let p = c.get::<Shared<MfStateProcess>>(process)?;
            let value = match kind {
                0 => p.reversion(),
                1 => p.x0()?,
                2 => p.drift(t, x)?,
                3 => p.diffusion(t, x)?,
                4 => p.expectation(t, x, dt)?,
                5 => p.std_deviation(t, x, dt)?,
                6 => p.variance(t, x, dt)?,
                7 => p.evolve(t, x, dt, dw)?,
                _ => return Err(BindingError::invalid("unknown MF state query kind")),
            };
            output(out, value)
        })
    }
}

/// Copies grid kind 0 times or 1 volatilities. Pass null output/capacity zero
/// to obtain its size. An undersized buffer leaves both outputs unchanged.
/// # Safety
/// Follow the crate-level C caller contract. Buffers and written must not overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_mf_state_process_grid(
    ctx: *mut Context,
    process: u64,
    kind: i32,
    out: *mut f64,
    capacity: usize,
    written: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(written)?;
            let p = c.get::<Shared<MfStateProcess>>(process)?;
            let values = match kind {
                0 => p.times(),
                1 => p.volatilities(),
                _ => return Err(BindingError::invalid("unknown MF state grid kind")),
            };
            if out.is_null() && capacity == 0 {
                return output(written, values.len());
            }
            check_ptr(out)?;
            if capacity < values.len() {
                return Err(BindingError::invalid("MF state grid buffer too small"));
            }
            for (i, value) in values.iter().enumerate() {
                out.add(i).write(*value);
            }
            output(written, values.len())
        })
    }
}
