//! GJR calibration using retained Heston helpers and existing optimizers.
use crate::bates_api::with_bates_context;
use crate::boundary::*;
use crate::constraint_api::{ItofinCalibrationOptions, read_options};
use libitofin::math::optimization::{endcriteria::EndCriteria, method::OptimizationMethod};
use libitofin::models::calibrationhelper::{BlackCalibrationHelper, CalibrationHelper};
use libitofin::models::equity::HestonModelHelper;
use libitofin::models::{GjrGarchModel, calibrate};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::vanilla::AnalyticGjrGarchEngine;
use libitofin::shared::{SharedMut, shared_mut};

/// Calibrate omega, alpha, beta, gamma, lambda, v0 with copied options.
/// The fixed-mask order matches model parameters. Helpers retain an analytic
/// GJR engine. Failed calibration preserves model parameters and diagnostics.
/// # Safety
/// Input arrays are read during this call only. Pointers and handles obey the
/// C caller contract; context and handles belong to the calling thread.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_gjr_calibrate_with_options(
    ctx: *mut Context,
    model: u64,
    helpers: *const u64,
    helpers_len: usize,
    method: u64,
    criteria: u64,
    options: *const ItofinCalibrationOptions,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_bates_context(ctx, error, |c| {
            let options = read_options(c, options)?;
            let ids = input_slice(helpers, helpers_len)?;
            if ids.is_empty() {
                return Err(BindingError::invalid(
                    "calibration helpers must not be empty",
                ));
            }
            let model = c.get::<SharedMut<GjrGarchModel>>(model)?;
            let method = c.get::<SharedMut<dyn OptimizationMethod>>(method)?;
            let criteria = c.get::<EndCriteria>(criteria)?;
            let helpers = ids
                .iter()
                .map(|id| c.get::<SharedMut<HestonModelHelper>>(*id))
                .collect::<BindingResult<Vec<_>>>()?;
            let engine: SharedMut<dyn PricingEngine> =
                shared_mut(AnalyticGjrGarchEngine::new(model.clone()));
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
                options.constraint,
                options.weights,
                options.fix_parameters,
            )?;
            Ok(())
        })
    }
}
