//! Named scalar geometric Brownian process with QuantLib Euler transitions.
use crate::boundary::*;
use libitofin::processes::GeometricBrownianMotionProcess;
use libitofin::shared::{Shared, shared};
use libitofin::stochasticprocess::StochasticProcess1D;

/// Create a scalar process with finite initial value/drift and nonnegative volatility.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_geometric_brownian_new(
    ctx: *mut Context,
    initial: f64,
    mu: f64,
    volatility: f64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let process = GeometricBrownianMotionProcess::new(initial, mu, volatility)?;
            output(out, c.insert(shared(process))?)
        })
    }
}

/// Kind: 0 initial, 1 mu, 2 volatility, 3 drift, 4 diffusion, 5 expectation,
/// 6 variance, 7 signed standard deviation, 8 Euler evolve with Gaussian `draw`.
/// State/time are used by 3-8; dt by 5-8; draw only by 8. Unused inputs are ignored.
/// # Safety
/// Follow the crate-level C caller contract. Output is written only on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_geometric_brownian_query(
    ctx: *mut Context,
    process: u64,
    kind: i32,
    t: f64,
    state: f64,
    dt: f64,
    draw: f64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let p = c.get::<Shared<GeometricBrownianMotionProcess>>(process)?;
            let value = match kind {
                0 => p.x0()?,
                1 => p.mu(),
                2 => p.volatility(),
                3 => p.drift(t, state)?,
                4 => p.diffusion(t, state)?,
                5 => p.expectation(t, state, dt)?,
                6 => p.variance(t, state, dt)?,
                7 => p.std_deviation(t, state, dt)?,
                8 => p.evolve(t, state, dt, draw)?,
                _ => {
                    return Err(BindingError::invalid(
                        "unknown geometric Brownian query kind",
                    ));
                }
            };
            output(out, value)
        })
    }
}

#[cfg(test)]
#[path = "geometric_brownian_api_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "geometric_brownian_oracle_tests.rs"]
mod oracle_tests;
