//! Explicit pseudo-random European GJR Monte Carlo configuration.
use crate::bates_api::with_bates_context;
use crate::boundary::*;
use libitofin::math::randomnumbers::rngtraits::PseudoRandom;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::vanilla::MakeMcEuropeanGjrGarchEngine;
use libitofin::processes::GjrGarchProcess;
use libitofin::shared::{Shared, SharedMut, shared_mut};

/// Zero means unset for step/sample/tolerance/max-sample fields. Set exactly
/// one step mode and one sampling mode. Seed is limited to u32; antithetic is
/// 0 or 1; seed 0 requests a randomized seed. Fixed samples require at least 2.
/// Tolerance mode defaults to 50,000 max samples and requires a maximum of at
/// least 1,023. Core checked work limits also apply. No control variate,
/// Brownian bridge or Sobol mode is supported.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ItofinGjrMcConfig {
    pub steps: usize,
    pub steps_per_year: usize,
    pub samples: usize,
    pub absolute_tolerance: f64,
    pub max_samples: usize,
    pub seed: u64,
    pub antithetic: i32,
}

fn validate(cfg: &ItofinGjrMcConfig) -> BindingResult<u32> {
    if (cfg.steps == 0) == (cfg.steps_per_year == 0) {
        return Err(BindingError::invalid("set exactly one GJR step mode"));
    }
    if !cfg.absolute_tolerance.is_finite() || cfg.absolute_tolerance < 0.0 {
        return Err(BindingError::invalid("invalid GJR absolute tolerance"));
    }
    if (cfg.samples == 0) == (cfg.absolute_tolerance == 0.0) {
        return Err(BindingError::invalid("set exactly one GJR sampling mode"));
    }
    if cfg.max_samples > 0 && cfg.samples > cfg.max_samples {
        return Err(BindingError::invalid("GJR samples exceed maximum samples"));
    }
    if !(0..=1).contains(&cfg.antithetic) {
        return Err(BindingError::invalid("antithetic must be 0 or 1"));
    }
    u32::try_from(cfg.seed).map_err(|_| BindingError::invalid("GJR seed exceeds u32 range"))
}

/// Retain a European GJR Monte Carlo engine, attachable with option kind 2.
/// Nonzero fixed-seed engines restart their random stream on every repricing.
/// # Safety
/// Config is read during this call only. Pointers and handles obey the C contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_gjr_mc_engine_new(
    ctx: *mut Context,
    process: u64,
    config: *const ItofinGjrMcConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_bates_context(ctx, error, |c| {
            check_ptr(config)?;
            check_ptr(out)?;
            let cfg = &*config;
            let seed = validate(cfg)?;
            let mut maker =
                MakeMcEuropeanGjrGarchEngine::<PseudoRandom>::new(
                    c.get::<Shared<GjrGarchProcess>>(process)?,
                )
                .with_seed(seed)
                .with_antithetic_variate(cfg.antithetic != 0);
            maker = if cfg.steps > 0 {
                maker.with_steps(cfg.steps)
            } else {
                maker.with_steps_per_year(cfg.steps_per_year)
            };
            maker = if cfg.samples > 0 {
                maker.with_samples(cfg.samples)
            } else {
                maker.with_absolute_tolerance(cfg.absolute_tolerance)
            };
            if cfg.max_samples > 0 {
                maker = maker.with_max_samples(cfg.max_samples);
            }
            let engine: SharedMut<dyn PricingEngine> = shared_mut(maker.build()?);
            output(out, c.insert(engine)?)
        })
    }
}
