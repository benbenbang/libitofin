//! PseudoRandom Monte Carlo engines; configuration preserves Python optionality.
use crate::boundary::*;
use libitofin::math::randomnumbers::rngtraits::PseudoRandom;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::vanilla::{
    MakeMcAmericanEngine, MakeMcEuropeanEngine, MakeMcEuropeanHestonEngine,
};
use libitofin::processes::{GeneralizedBlackScholesProcess, HestonProcess};
use libitofin::shared::{Shared, SharedMut, shared_mut};

/// Presence bits: steps=1, steps/year=2, samples=4, tolerance=8, max_samples=16,
/// seed=32, antithetic=64, polynomial_order=128, calibration_samples=256.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct McConfig {
    pub present: u32,
    pub steps: usize,
    pub steps_per_year: usize,
    pub samples: usize,
    pub absolute_tolerance: f64,
    pub max_samples: usize,
    pub seed: u32,
    pub antithetic: i32,
    pub polynomial_order: usize,
    pub calibration_samples: usize,
}
macro_rules! configure {
    ($maker:expr, $cfg:expr) => {{
        let mut maker = $maker;
        let cfg = $cfg;
        if cfg.present & 1 != 0 {
            maker = maker.with_steps(cfg.steps);
        }
        if cfg.present & 2 != 0 {
            maker = maker.with_steps_per_year(cfg.steps_per_year);
        }
        if cfg.present & 4 != 0 {
            maker = maker.with_samples(cfg.samples);
        }
        if cfg.present & 8 != 0 {
            maker = maker.with_absolute_tolerance(cfg.absolute_tolerance);
        }
        if cfg.present & 16 != 0 {
            maker = maker.with_max_samples(cfg.max_samples);
        }
        if cfg.present & 32 != 0 {
            maker = maker.with_seed(cfg.seed);
        }
        if cfg.present & 64 != 0 {
            maker = maker.with_antithetic_variate(cfg.antithetic != 0);
        }
        maker
    }};
}
/// Kind 0 European BSM, 1 European Heston, 2 American BSM.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_mc_engine_new(
    ctx: *mut Context,
    process: u64,
    kind: i32,
    cfg: McConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if cfg.present & !511 != 0
                || (cfg.present & 64 != 0 && !(0..=1).contains(&cfg.antithetic))
            {
                return Err(BindingError::invalid("invalid MC presence mask or boolean"));
            }
            if kind != 2 && cfg.present & (128 | 256) != 0 {
                return Err(BindingError::invalid(
                    "regression settings require American engine",
                ));
            }
            let engine: SharedMut<dyn PricingEngine> = match kind {
                0 => shared_mut(
                    configure!(
                        MakeMcEuropeanEngine::<PseudoRandom>::new(c.get::<Shared<
                            GeneralizedBlackScholesProcess,
                        >>(
                            process
                        )?),
                        cfg
                    )
                    .build()?,
                ),
                1 => shared_mut(
                    configure!(
                        MakeMcEuropeanHestonEngine::<PseudoRandom>::new(
                            c.get::<Shared<HestonProcess>>(process)?
                        ),
                        cfg
                    )
                    .build()?,
                ),
                2 => {
                    let mut maker = configure!(
                        MakeMcAmericanEngine::<PseudoRandom>::new(c.get::<Shared<
                            GeneralizedBlackScholesProcess,
                        >>(
                            process
                        )?),
                        cfg
                    );
                    if cfg.present & 128 != 0 {
                        maker = maker.with_polynomial_order(cfg.polynomial_order);
                    }
                    if cfg.present & 256 != 0 {
                        maker = maker.with_calibration_samples(cfg.calibration_samples);
                    }
                    shared_mut(maker.build()?)
                }
                _ => return Err(BindingError::invalid("unknown MC engine kind")),
            };
            output(out, c.insert(engine)?)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::null_mut;
    #[test]
    fn invalid_masks_and_null_outputs_are_rejected_before_handle_access() {
        let mut context = Context::new();
        let mut out = 77;
        unsafe {
            assert_eq!(
                itofin_mc_engine_new(
                    &mut context,
                    0,
                    0,
                    McConfig::default(),
                    null_mut(),
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_mc_engine_new(
                    &mut context,
                    0,
                    0,
                    McConfig {
                        present: 1024,
                        ..McConfig::default()
                    },
                    &mut out,
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_mc_engine_new(
                    &mut context,
                    0,
                    0,
                    McConfig {
                        present: 64,
                        antithetic: 2,
                        ..McConfig::default()
                    },
                    &mut out,
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_mc_engine_new(
                    &mut context,
                    0,
                    0,
                    McConfig {
                        present: 128,
                        ..McConfig::default()
                    },
                    &mut out,
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
        }
        assert_eq!(out, 77);
    }
}
