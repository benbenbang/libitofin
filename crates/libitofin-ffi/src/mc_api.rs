//! Concrete Monte Carlo engines; configuration preserves Python optionality.
use crate::boundary::*;
use libitofin::math::randomnumbers::rngtraits::{LowDiscrepancy, PseudoRandom};
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
/// Kind 0 European BSM, 1 European Heston, 2 American BSM, 3 Sobol European BSM.
/// Kind 3 requires positive fixed samples/steps and rejects max_samples.
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
                3 => shared_mut(
                    configure!(
                        MakeMcEuropeanEngine::<LowDiscrepancy>::new(c.get::<Shared<
                            GeneralizedBlackScholesProcess,
                        >>(
                            process
                        )?),
                        cfg
                    )
                    .build()?,
                ),
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
    #[test]
    fn qmc_invalid_inputs_preserve_output_and_recover() {
        use crate::market_api::itofin_black_scholes_new;
        use crate::options_api::{
            itofin_option_new, itofin_option_set_engine, itofin_option_value,
        };
        use libitofin::settings::Settings;
        use libitofin::shared::shared;
        use libitofin::time::date::{Date, Month};
        use libitofin::time::daycounters::actual360::Actual360;

        let mut c = Context::new();
        let today = Date::new(15, Month::June, 2026);
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today);
        let settings = c.insert(settings).unwrap();
        let dc = c.insert(Actual360::new()).unwrap();
        let mut process = 0;
        let mut out = 77;
        let cfg = McConfig {
            present: 1 | 4,
            steps: 1,
            samples: 4095,
            ..McConfig::default()
        };
        unsafe {
            assert_eq!(
                itofin_black_scholes_new(
                    &mut c,
                    100.0,
                    0.05,
                    0.02,
                    0.2,
                    today.serial_number(),
                    dc,
                    &mut process,
                    null_mut()
                ),
                0
            );
            for bad in [
                McConfig {
                    present: 1024,
                    ..cfg
                },
                McConfig {
                    present: 1 | 4 | 64,
                    antithetic: 2,
                    ..cfg
                },
                McConfig {
                    present: 1 | 4 | 128,
                    ..cfg
                },
                McConfig { present: 4, ..cfg },
                McConfig { present: 1, ..cfg },
                McConfig { steps: 0, ..cfg },
                McConfig { samples: 0, ..cfg },
                McConfig {
                    present: 1 | 4 | 8,
                    absolute_tolerance: 0.01,
                    ..cfg
                },
                McConfig {
                    present: 1 | 4 | 16,
                    max_samples: 8191,
                    ..cfg
                },
            ] {
                assert_ne!(
                    itofin_mc_engine_new(&mut c, process, 3, bad, &mut out, null_mut()),
                    0
                );
                assert_eq!(out, 77);
            }
            if let Some(samples) = (u32::MAX as usize).checked_add(1) {
                let bad = McConfig { samples, ..cfg };
                assert_ne!(
                    itofin_mc_engine_new(&mut c, process, 3, bad, &mut out, null_mut()),
                    0
                );
                assert_eq!(out, 77);
            }
            assert_ne!(
                itofin_mc_engine_new(&mut c, process, 3, cfg, null_mut(), null_mut()),
                0
            );
            assert_ne!(
                itofin_mc_engine_new(&mut c, dc, 3, cfg, &mut out, null_mut()),
                0
            );
            assert_eq!(out, 77);
            assert_eq!(
                itofin_mc_engine_new(&mut c, process, 3, cfg, &mut out, null_mut()),
                0
            );
            let mut option = 0;
            assert_eq!(
                itofin_option_new(
                    &mut c,
                    0,
                    100.0,
                    0,
                    today.serial_number() + 360,
                    0,
                    settings,
                    &mut option,
                    null_mut()
                ),
                0
            );
            assert_eq!(
                itofin_option_set_engine(&mut c, option, out, 2, 0, null_mut()),
                0
            );
            let mut value = f64::NAN;
            assert_eq!(
                itofin_option_value(&mut c, option, 0, &mut value, null_mut()),
                0
            );
            assert!(value.is_finite());
            value = 123.0;
            assert_ne!(
                itofin_option_value(&mut c, option, 7, &mut value, null_mut()),
                0
            );
            assert_eq!(value, 123.0);
            assert_eq!(
                itofin_option_value(&mut c, option, 0, &mut value, null_mut()),
                0
            );
            assert!(value.is_finite());
        }
    }
}
