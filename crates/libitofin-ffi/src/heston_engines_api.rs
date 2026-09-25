//! Alternative Heston engines and additive calibration entrypoints.
use crate::boundary::*;
use libitofin::math::optimization::{endcriteria::EndCriteria, method::OptimizationMethod};
use libitofin::models::calibrationhelper::{BlackCalibrationHelper, CalibrationHelper};
use libitofin::models::equity::HestonModelHelper;
use libitofin::models::{HestonModel, calibrate};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::vanilla::coshestonengine::CosHestonEngine;
use libitofin::pricingengines::vanilla::exponentialfittinghestonengine::{
    ExponentialFittingControlVariate, ExponentialFittingHestonEngine,
};
use libitofin::shared::{SharedMut, shared_mut};

/// Alternative Heston engine options. Kind 0 COS, 1 exponential fitting.
/// CV 0 Optimal, 1 AndersenPiterbarg, 2 AndersenPiterbargOptCV,
/// 3 AsymptoticChF, 4 AngledContour, 5 AngledContourNoCV.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ItofinHestonEngineConfig {
    pub kind: i32,
    pub l: f64,
    pub n: usize,
    pub control_variate: i32,
    pub has_scaling: i32,
    pub scaling: f64,
    pub alpha: f64,
}
fn engine(
    model: SharedMut<HestonModel>,
    cfg: &ItofinHestonEngineConfig,
) -> BindingResult<SharedMut<dyn PricingEngine>> {
    match cfg.kind {
        0 => Ok(shared_mut(CosHestonEngine::new(model, cfg.l, cfg.n)?)),
        1 => {
            let cv = match cfg.control_variate {
                0 => ExponentialFittingControlVariate::Optimal,
                1 => ExponentialFittingControlVariate::AndersenPiterbarg,
                2 => ExponentialFittingControlVariate::AndersenPiterbargOptCV,
                3 => ExponentialFittingControlVariate::AsymptoticChF,
                4 => ExponentialFittingControlVariate::AngledContour,
                5 => ExponentialFittingControlVariate::AngledContourNoCV,
                _ => {
                    return Err(BindingError::invalid(
                        "unknown exponential fitting control variate",
                    ));
                }
            };
            let scaling = match cfg.has_scaling {
                0 => None,
                1 => Some(cfg.scaling),
                _ => return Err(BindingError::invalid("has_scaling must be 0 or 1")),
            };
            Ok(shared_mut(ExponentialFittingHestonEngine::new(
                model, cv, scaling, cfg.alpha,
            )?))
        }
        _ => Err(BindingError::invalid("unknown Heston engine kind")),
    }
}

/// Create a retained engine usable with option engine kind 2.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Context and handles belong to the
/// calling thread; serialize calls including destruction.
pub unsafe extern "C" fn itofin_heston_engine_new(
    ctx: *mut Context,
    model: u64,
    config: *const ItofinHestonEngineConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(config)?;
            check_ptr(out)?;
            let model = c.get::<SharedMut<HestonModel>>(model)?;
            let cfg = &*config;
            let id = if cfg.kind == 0 {
                c.insert(shared_mut(CosHestonEngine::new(model, cfg.l, cfg.n)?))?
            } else {
                c.insert(engine(model, cfg)?)?
            };
            output(out, id)
        })
    }
}

/// Calibrate a Heston model using the configured alternative engine.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Context and handles belong to the
/// calling thread; serialize calls including destruction.
pub unsafe extern "C" fn itofin_heston_calibrate_engine(
    ctx: *mut Context,
    model: u64,
    helpers: *const u64,
    helpers_len: usize,
    method: u64,
    criteria: u64,
    config: *const ItofinHestonEngineConfig,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(config)?;
            let ids = input_slice(helpers, helpers_len)?;
            if ids.is_empty() {
                return Err(BindingError::invalid(
                    "calibration helpers must not be empty",
                ));
            }
            let model = c.get::<SharedMut<HestonModel>>(model)?;
            let method = c.get::<SharedMut<dyn OptimizationMethod>>(method)?;
            let criteria = c.get::<EndCriteria>(criteria)?;
            let helpers = ids
                .iter()
                .map(|id| c.get::<SharedMut<HestonModelHelper>>(*id))
                .collect::<BindingResult<Vec<_>>>()?;
            let engine = engine(model.clone(), &*config)?;
            for helper in &helpers {
                helper
                    .borrow_mut()
                    .base_mut()
                    .set_pricing_engine(engine.clone());
            }
            let helpers: Vec<SharedMut<dyn CalibrationHelper>> = helpers
                .into_iter()
                .map(|h| h as SharedMut<dyn CalibrationHelper>)
                .collect();
            calibrate(
                &model,
                &helpers,
                &mut *method.borrow_mut(),
                &criteria,
                None,
                vec![],
                vec![],
            )?;
            Ok(())
        })
    }
}

/// COS inspector field: 0-3 cumulants, 4 log forward/spot, 5 characteristic function.
/// Time must be finite and nonnegative; frequency must be finite. Scalar fields
/// write zero to the imaginary output. The engine retains its live model.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid. Outputs must not overlap each other.
/// Context and handles belong to the calling thread; serialize all calls.
pub unsafe extern "C" fn itofin_cos_heston_value(
    ctx: *mut Context,
    engine: u64,
    field: i32,
    t: f64,
    u: f64,
    out_real: *mut f64,
    out_imag: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out_real)?;
            check_ptr(out_imag)?;
            if !t.is_finite() || t < 0.0 || !u.is_finite() {
                return Err(BindingError::invalid(
                    "finite nonnegative time and finite frequency required",
                ));
            }
            let engine = c.get::<SharedMut<CosHestonEngine>>(engine)?;
            let engine = engine.borrow();
            let (real, imag) = match field {
                0..=3 if t == 0.0 => (0.0, 0.0),
                0 => (engine.c1(t), 0.0),
                1 => (engine.c2(t), 0.0),
                2 => (engine.c3(t), 0.0),
                3 => (engine.c4(t), 0.0),
                4 => (engine.mu_t(t)?, 0.0),
                5 => {
                    let value = engine.chf(u, t);
                    (value.re, value.im)
                }
                _ => return Err(BindingError::invalid("unknown COS inspector field")),
            };
            if !real.is_finite() || !imag.is_finite() {
                return Err(BindingError::invalid("COS inspector result is not finite"));
            }
            output(out_real, real)?;
            output(out_imag, imag)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libitofin::handle::Handle;
    use libitofin::interestrate::Compounding;
    use libitofin::processes::HestonProcess;
    use libitofin::quotes::{Quote, SimpleQuote};
    use libitofin::shared::{Shared, shared};
    use libitofin::termstructures::yields::FlatForward;
    use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
    use libitofin::time::date::{Date, Month};
    use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
    use libitofin::time::frequency::Frequency;

    fn model() -> SharedMut<HestonModel> {
        let reference = Date::new(7, Month::February, 2017);
        let flat = |rate| {
            Handle::new(shared(FlatForward::with_rate(
                reference,
                rate,
                Actual365Fixed::new(),
                Compounding::Continuous,
                Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>)
        };
        HestonModel::new(shared(HestonProcess::new(
            flat(0.15),
            flat(0.07),
            Handle::new(shared(SimpleQuote::new(100.0)) as Shared<dyn Quote>),
            0.1,
            4.0,
            0.22,
            1.8,
            -0.75,
        )))
        .unwrap()
    }

    #[test]
    fn heston_engine_config_rejects_unknown_discriminants_with_valid_market() {
        let model = model();
        let valid = ItofinHestonEngineConfig {
            kind: 1,
            l: 16.0,
            n: 200,
            control_variate: 0,
            has_scaling: 0,
            scaling: 1.0,
            alpha: -0.5,
        };
        assert!(engine(model.clone(), &valid).is_ok());
        assert!(
            engine(
                model.clone(),
                &ItofinHestonEngineConfig { kind: 2, ..valid }
            )
            .is_err()
        );
        assert!(
            engine(
                model.clone(),
                &ItofinHestonEngineConfig {
                    control_variate: 6,
                    ..valid
                }
            )
            .is_err()
        );
        assert!(
            engine(
                model.clone(),
                &ItofinHestonEngineConfig {
                    has_scaling: 2,
                    ..valid
                }
            )
            .is_err()
        );
        assert!(
            engine(
                model.clone(),
                &ItofinHestonEngineConfig {
                    has_scaling: 1,
                    scaling: 0.0,
                    ..valid
                }
            )
            .is_err()
        );
        assert!(engine(model, &valid).is_ok());
    }
    #[test]
    fn cos_inspector_failures_preserve_both_outputs_and_allow_recovery() {
        use std::ptr::null_mut;
        let mut context = Context::new();
        let model = model();
        let wrong = context.insert(model.clone()).unwrap();
        let id = context
            .insert(shared_mut(CosHestonEngine::new(model, 16.0, 200).unwrap()))
            .unwrap();
        let (mut real, mut imag) = (123.0, 456.0);
        unsafe {
            for (handle, field, status) in [(wrong, 0, INVALID_HANDLE), (id, 6, INVALID_ARGUMENT)] {
                assert_eq!(
                    itofin_cos_heston_value(
                        &mut context,
                        handle,
                        field,
                        1.0,
                        0.5,
                        &mut real,
                        &mut imag,
                        null_mut()
                    ),
                    status
                );
                assert_eq!((real, imag), (123.0, 456.0));
            }
            assert_eq!(
                itofin_cos_heston_value(
                    &mut context,
                    id,
                    5,
                    1.0,
                    0.5,
                    &mut real,
                    null_mut(),
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!((real, imag), (123.0, 456.0));
            assert_eq!(
                itofin_cos_heston_value(
                    &mut context,
                    id,
                    0,
                    1.0,
                    0.0,
                    &mut real,
                    &mut imag,
                    null_mut()
                ),
                0
            );
            assert!(real.is_finite() && real < 0.0);
            assert_eq!(imag, 0.0);
        }
    }
}
