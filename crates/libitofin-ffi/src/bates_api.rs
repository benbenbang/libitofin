//! Live Bates processes, calibrated models and European analytic engines.
use crate::boundary::*;
use crate::market_api::quote;
use crate::time_api::date;
use libitofin::handle::Handle;
use libitofin::math::array::Array;
use libitofin::models::{BatesModel, CalibratedModelHolder};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::vanilla::BatesEngine;
use libitofin::processes::BatesProcess;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;

/// Physical constructor order, distinct from calibrated parameter-array order.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ItofinBatesParameters {
    pub v0: f64,
    pub kappa: f64,
    pub theta: f64,
    pub sigma: f64,
    pub rho: f64,
    pub lambda: f64,
    pub nu: f64,
    pub delta: f64,
}

/// Retain live spot, risk-free and dividend inputs with constant parameters.
/// # Safety
/// Pointers must be aligned, live and non-overlapping. Context and handles
/// belong to the calling thread; serialize calls including destruction.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_bates_process_new(
    ctx: *mut Context,
    spot: u64,
    risk_free: u64,
    dividend: u64,
    parameters: *const ItofinBatesParameters,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_bates_context(ctx, error, |c| {
            check_ptr(parameters)?;
            check_ptr(out)?;
            let p = &*parameters;
            let process = BatesProcess::new(
                c.get::<Handle<dyn YieldTermStructure>>(risk_free)?,
                c.get::<Handle<dyn YieldTermStructure>>(dividend)?,
                quote(c, spot)?,
                p.v0,
                p.kappa,
                p.theta,
                p.sigma,
                p.rho,
                p.lambda,
                p.nu,
                p.delta,
            )?;
            output(out, c.insert(shared(process))?)
        })
    }
}

/// Retain the process and seed eight calibrated parameters.
/// # Safety
/// Pointers, context and handles must obey the C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_bates_model_new(
    ctx: *mut Context,
    process: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_bates_context(ctx, error, |c| {
            check_ptr(out)?;
            let model = BatesModel::new(c.get::<Shared<BatesProcess>>(process)?)?;
            output(out, c.insert(model)?)
        })
    }
}

/// Kind 0 process, 1 model; field 0 v0, 1 kappa, 2 theta, 3 sigma,
/// 4 rho, 5 lambda, 6 nu, 7 delta.
/// # Safety
/// Pointers, context and handles must obey the C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_bates_parameter(
    ctx: *mut Context,
    handle: u64,
    kind: i32,
    field: usize,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_bates_context(ctx, error, |c| {
            check_ptr(out)?;
            let process = match kind {
                0 => c.get::<Shared<BatesProcess>>(handle)?,
                1 => c.get::<SharedMut<BatesModel>>(handle)?.borrow().process(),
                _ => return Err(BindingError::invalid("unknown Bates handle kind")),
            };
            let value = match field {
                0 => process.v0(),
                1 => process.kappa(),
                2 => process.theta(),
                3 => process.sigma(),
                4 => process.rho(),
                5 => process.lambda(),
                6 => process.nu(),
                7 => process.delta(),
                _ => return Err(BindingError::invalid("unknown Bates parameter field")),
            };
            output(out, value)
        })
    }
}

/// Convert a date with the retained risk-free curve's clock.
/// # Safety
/// Pointers, context and handles must obey the C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_bates_process_time(
    ctx: *mut Context,
    process: u64,
    date_serial: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_bates_context(ctx, error, |c| {
            check_ptr(out)?;
            let process = c.get::<Shared<BatesProcess>>(process)?;
            output(out, process.time(&date(date_serial)?)?)
        })
    }
}

/// Copy the current spot and initial variance, without partial writes on error.
/// # Safety
/// `out` must hold two writable doubles. Other arguments obey the C contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_bates_process_initial_values(
    ctx: *mut Context,
    process: u64,
    out: *mut f64,
    capacity: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_bates_context(ctx, error, |c| {
            check_ptr(out)?;
            if capacity < 2 {
                return Err(BindingError::invalid(
                    "Bates initial-value capacity must be at least 2",
                ));
            }
            let values = c.get::<Shared<BatesProcess>>(process)?.initial_values()?;
            std::ptr::copy_nonoverlapping(values.as_ptr(), out, 2);
            Ok(())
        })
    }
}

/// Copy theta,kappa,sigma,rho,v0,nu,delta,lambda atomically.
/// # Safety
/// `out` must hold eight writable doubles. Other arguments obey the C contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_bates_model_params(
    ctx: *mut Context,
    model: u64,
    out: *mut f64,
    capacity: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_bates_context(ctx, error, |c| {
            check_ptr(out)?;
            if capacity < 8 {
                return Err(BindingError::invalid(
                    "Bates parameter capacity must be at least 8",
                ));
            }
            let model = c.get::<SharedMut<BatesModel>>(model)?;
            let values = model.borrow().calibrated_model().params();
            std::ptr::copy_nonoverlapping(values.as_ptr(), out, 8);
            Ok(())
        })
    }
}

/// Validate and atomically replace theta,kappa,sigma,rho,v0,nu,delta,lambda.
/// Input is copied and never retained.
/// # Safety
/// `parameters` must hold `len` readable doubles. Other arguments obey the C contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_bates_model_set_params(
    ctx: *mut Context,
    model: u64,
    parameters: *const f64,
    len: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_bates_context(ctx, error, |c| {
            if len != 8 {
                return Err(BindingError::invalid(
                    "Bates requires exactly eight parameters",
                ));
            }
            let parameters = Array::from(input_slice(parameters, len)?.to_vec());
            c.get::<SharedMut<BatesModel>>(model)?
                .borrow_mut()
                .set_params(&parameters)?;
            Ok(())
        })
    }
}

/// Retain a Bates engine for option engine kind 2. Orders 1..192;
/// 144 is conventional. European plain-vanilla NPV only, no Greeks.
/// # Safety
/// Pointers, context and handles must obey the C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_bates_engine_new(
    ctx: *mut Context,
    model: u64,
    integration_order: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_bates_context(ctx, error, |c| {
            check_ptr(out)?;
            let engine: SharedMut<dyn PricingEngine> = shared_mut(BatesEngine::new(
                c.get::<SharedMut<BatesModel>>(model)?,
                integration_order,
            )?);
            output(out, c.insert(engine)?)
        })
    }
}

/// # Safety
/// Context and error obey the C caller contract; errors are validated before mutation.
/// Reject misaligned error storage before any output or model mutation.
/// # Safety
/// Context, pointers and closure operations must obey the C caller contract.
pub(crate) unsafe fn with_bates_context(
    ctx: *mut Context,
    error: *mut ItofinError,
    f: impl FnOnce(&mut Context) -> BindingResult<()>,
) -> i32 {
    if !error.is_null() && check_ptr(error).is_err() {
        return INVALID_ARGUMENT;
    }
    unsafe { with_context(ctx, error, f) }
}
