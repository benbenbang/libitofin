//! Calibrated GJR-GARCH models and retained European pricing engines.
use crate::bates_api::with_bates_context;
use crate::boundary::*;
use libitofin::math::array::Array;
use libitofin::models::{CalibratedModelHolder, GjrGarchModel};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::vanilla::AnalyticGjrGarchEngine;
use libitofin::processes::GjrGarchProcess;
use libitofin::shared::{Shared, SharedMut, shared_mut};

pub mod calibration;
pub mod mc;

/// Retain the live process inputs and seed six daily model parameters.
/// # Safety
/// Pointers and thread-confined handles obey the crate-level C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_gjr_model_new(
    ctx: *mut Context,
    process: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_bates_context(ctx, error, |c| {
            check_ptr(out)?;
            let model = GjrGarchModel::new(c.get::<Shared<GjrGarchProcess>>(process)?)?;
            output(out, c.insert(model)?)
        })
    }
}

/// Copy omega, alpha, beta, gamma, lambda, v0 without partial writes on error.
/// Variance and omega are daily quantities, not annualized values.
/// # Safety
/// Output holds `capacity` writable doubles; pointers obey the C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_gjr_model_params(
    ctx: *mut Context,
    model: u64,
    out: *mut f64,
    capacity: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_bates_context(ctx, error, |c| {
            check_ptr(out)?;
            if capacity < 6 || capacity > isize::MAX as usize / size_of::<f64>() {
                return Err(BindingError::invalid("invalid GJR parameter capacity"));
            }
            let model = c.get::<SharedMut<GjrGarchModel>>(model)?;
            let values = model.borrow().calibrated_model().params();
            std::ptr::copy_nonoverlapping(values.as_ptr(), out, 6);
            Ok(())
        })
    }
}

/// Atomically replace omega, alpha, beta, gamma, lambda, v0.
/// Inputs are copied, never retained; invalid parameters leave the model unchanged.
/// # Safety
/// Input holds `len` readable doubles; pointers obey the C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_gjr_model_set_params(
    ctx: *mut Context,
    model: u64,
    parameters: *const f64,
    len: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_bates_context(ctx, error, |c| {
            if len != 6 {
                return Err(BindingError::invalid("GJR requires exactly six parameters"));
            }
            let parameters = Array::from(input_slice(parameters, len)?.to_vec());
            c.get::<SharedMut<GjrGarchModel>>(model)?
                .borrow_mut()
                .set_params(&parameters)?;
            Ok(())
        })
    }
}

/// Retain the current generated process, usable with all GJR process APIs.
/// The returned process is a snapshot: later parameter updates replace the
/// model's process but do not change this retained process's parameters.
/// Live market handles remain shared; generated processes use FullTruncation.
/// # Safety
/// Pointers and thread-confined handles obey the crate-level C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_gjr_model_process(
    ctx: *mut Context,
    model: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_bates_context(ctx, error, |c| {
            check_ptr(out)?;
            let process = c.get::<SharedMut<GjrGarchModel>>(model)?.borrow().process();
            output(out, c.insert(process)?)
        })
    }
}

/// Retain an analytic European engine, attachable with option engine kind 2.
/// Only European plain-vanilla NPV is supported; Greeks are unavailable.
/// # Safety
/// Pointers and thread-confined handles obey the crate-level C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_gjr_analytic_engine_new(
    ctx: *mut Context,
    model: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_bates_context(ctx, error, |c| {
            check_ptr(out)?;
            let engine: SharedMut<dyn PricingEngine> = shared_mut(AnalyticGjrGarchEngine::new(
                c.get::<SharedMut<GjrGarchModel>>(model)?,
            ));
            output(out, c.insert(engine)?)
        })
    }
}
