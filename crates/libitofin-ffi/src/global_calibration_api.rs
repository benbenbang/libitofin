//! Bounded differential evolution as a retained model-calibration method.

use crate::boundary::*;
use crate::optimize_api::{
    ItofinDifferentialEvolutionOptions, ItofinOptimizeResult, decode_differential_evolution, status,
};
use itofin_optimize::Bounds;
use libitofin::math::optimization::global::DifferentialEvolution;
use libitofin::math::optimization::method::OptimizationMethod;
use libitofin::shared::{SharedMut, shared_mut};

/// Construct a calibration method in projected free-parameter order.
/// A zeroed options record selects defaults, including deterministic seed zero.
/// Population is optional row-major `rows * n`, with exact length required.
/// Every candidate must satisfy the model constraint before its cost is called.
/// # Safety
/// Pointers must satisfy the crate-level C caller contract.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_differential_evolution_new(
    ctx: *mut Context,
    lower: *const f64,
    lower_len: usize,
    upper: *const f64,
    upper_len: usize,
    options: *const ItofinDifferentialEvolutionOptions,
    population: *const f64,
    population_rows: usize,
    population_len: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |context| {
            check_ptr(out)?;
            if lower_len == 0 || lower_len != upper_len {
                return Err(BindingError::invalid(
                    "nonempty equally sized bounds are required",
                ));
            }
            if lower_len > 256 || population_rows > 4096 || population_len > 1_000_000 {
                return Err(BindingError::invalid(
                    "calibration population allocation limit exceeded",
                ));
            }
            let expected = population_rows
                .checked_mul(lower_len)
                .ok_or_else(|| BindingError::invalid("population shape overflow"))?;
            if expected != population_len {
                return Err(BindingError::invalid(
                    "population must contain rows times dimension values",
                ));
            }
            let bounds = Bounds {
                lower: input_slice(lower, lower_len)?.to_vec(),
                upper: input_slice(upper, upper_len)?.to_vec(),
            };
            let rows = if population_rows == 0 {
                None
            } else {
                Some(
                    input_slice(population, population_len)?
                        .chunks_exact(lower_len)
                        .map(<[f64]>::to_vec)
                        .collect(),
                )
            };
            let options = if options.is_null() {
                ItofinDifferentialEvolutionOptions::default()
            } else {
                check_ptr(options)?;
                *options
            };
            let (options, common) = decode_differential_evolution(options, rows)?;
            let method = DifferentialEvolution::new(bounds, options, common)?;
            let method = shared_mut(method) as SharedMut<dyn OptimizationMethod>;
            output(out, context.insert(method)?)
        })
    }
}

/// Copy the preserved global outcome without evaluating or changing the model.
/// Returns an invalid-argument error when the handle has no completed global run.
/// # Safety
/// `out` must point to a valid result record whose `x` is writable for `n` doubles.
/// All pointers must satisfy the crate-level C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_differential_evolution_result(
    ctx: *mut Context,
    method: u64,
    n: usize,
    out: *mut ItofinOptimizeResult,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |context| {
            check_ptr(out)?;
            let method = context.get::<SharedMut<dyn OptimizationMethod>>(method)?;
            let method = method.borrow();
            let result = method
                .global_result()
                .ok_or_else(|| BindingError::invalid("method has no completed global result"))?;
            if n != result.x.len() {
                return Err(BindingError::invalid(
                    "result buffer length must match free parameter count",
                ));
            }
            let x = (*out).x;
            check_ptr(x)?;
            let result_status = status(result.status)?;
            std::ptr::copy_nonoverlapping(result.x.as_ptr(), x, n);
            output(
                out,
                ItofinOptimizeResult {
                    x,
                    fun: result.fun,
                    nit: result.nit,
                    nfev: result.nfev,
                    njev: result.njev,
                    status: result_status,
                    success: result.success,
                },
            )
        })
    }
}
