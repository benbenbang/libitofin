//! Bounded pseudo-random variance-swap simulation and sampling statistics.
use super::*;
use libitofin::pricingengines::MCVarianceSwapEngine;

/// Zero denotes an unset option. Exactly one grid and one sampling mode is required.
/// Tolerance measures annualized variance standard error, not monetary NPV error.
/// Seed zero is randomized; nonzero seeds must fit uint32.
/// Tolerance mode defaults to 50,000 maximum samples.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ItofinVarianceSwapMcConfig {
    pub steps: usize,
    pub steps_per_year: usize,
    pub samples: usize,
    pub absolute_tolerance: f64,
    pub max_samples: usize,
    pub seed: u64,
}

/// Retain the generalized Black-Scholes process and construct a typed MC engine.
/// No antithetic, bridge or discrete squared-return estimator is selected.
/// # Safety
/// Config and output pointers follow the crate-level C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_mc_variance_swap_engine_new(
    ctx: *mut Context,
    process: u64,
    config: *const ItofinVarianceSwapMcConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    if !valid_error(error) {
        return INVALID_ARGUMENT;
    }
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(config)?;
            check_ptr(out)?;
            let config = *config;
            let engine: SharedMut<dyn PricingEngine> = shared_mut(MCVarianceSwapEngine::new(
                c.get::<Shared<GeneralizedBlackScholesProcess>>(process)?,
                (config.steps != 0).then_some(config.steps),
                (config.steps_per_year != 0).then_some(config.steps_per_year),
                (config.samples != 0).then_some(config.samples),
                (config.absolute_tolerance != 0.).then_some(config.absolute_tolerance),
                (config.max_samples != 0).then_some(config.max_samples),
                config.seed,
            )?);
            output(out, c.insert(EngineOwner { engine, count: 0 })?)
        })
    }
}

/// Return actual MC samples. Replication and expired sampling results are unavailable.
/// # Safety
/// Output follows the C caller contract and remains unchanged on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_variance_swap_samples(
    ctx: *mut Context,
    swap: u64,
    out: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    if !valid_error(error) {
        return INVALID_ARGUMENT;
    }
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let owner = c.get::<SharedMut<SwapOwner>>(swap)?;
            output(out, owner.borrow_mut().swap.samples()?)
        })
    }
}

#[cfg(test)]
#[path = "mc_variance_swap_api_tests.rs"]
mod tests;
