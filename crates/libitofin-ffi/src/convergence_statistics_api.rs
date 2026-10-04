//! Context-owned mean-convergence diagnostics and stateless batch evaluation.

use crate::boundary::{
    BindingError, BindingResult, Context, ItofinError, check_ptr, input_slice, with_context,
    without_context,
};
use libitofin::math::statistics::{
    ConvergencePoint, ConvergenceStatistics, evaluate_convergence_batch,
    validate_convergence_length,
};
use libitofin::shared::{SharedMut, shared_mut};
use libitofin::types::Real;

fn check_error(error: *mut ItofinError) -> BindingResult<()> {
    if !error.is_null() {
        check_ptr(error)?;
    }
    Ok(())
}

unsafe fn optional_weights<'a>(
    weights: *const Real,
    len: usize,
    count: usize,
) -> BindingResult<Option<&'a [Real]>> {
    if weights.is_null() && len == 0 {
        return Ok(None);
    }
    if len != count {
        return Err(BindingError::invalid(
            "convergence observation and weight lengths differ",
        ));
    }
    check_ptr(weights)?;
    Ok(Some(unsafe { input_slice(weights, len)? }))
}

fn check_table_outputs(
    counts: *mut usize,
    means: *mut Real,
    len: usize,
    expected: usize,
) -> BindingResult<()> {
    if len != expected {
        return Err(BindingError::invalid(
            "convergence table output length mismatch",
        ));
    }
    if expected > 0 {
        check_ptr(counts)?;
        check_ptr(means)?;
    }
    Ok(())
}

unsafe fn write_table(points: &[ConvergencePoint], counts: *mut usize, means: *mut Real) {
    for (i, point) in points.iter().enumerate() {
        unsafe {
            counts.add(i).write(point.samples);
            means.add(i).write(point.mean);
        }
    }
}

/// Evaluate running means at completed checkpoints 1, 3, 7, 15, ... .
/// Null weights with zero length selects unit weights. Inputs are limited to
/// 100,000 observations. The output length must equal the number of completed
/// checkpoints; empty input requires length zero. All outputs stay unchanged
/// on error. Each completed checkpoint requires positive cumulative weight.
/// # Safety
/// Inputs and output arrays follow the crate-level pointer contract.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_convergence_statistics_evaluate(
    values: *const Real,
    len: usize,
    weights: *const Real,
    weights_len: usize,
    out_counts: *mut usize,
    out_means: *mut Real,
    out_len: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            check_error(error)?;
            let expected = validate_convergence_length(len)?;
            check_table_outputs(out_counts, out_means, out_len, expected)?;
            let weights = optional_weights(weights, weights_len, len)?;
            let result = evaluate_convergence_batch(input_slice(values, len)?, weights)?;
            write_table(&result, out_counts, out_means);
            Ok(())
        })
    }
}

/// Create an empty, context-owned accumulator. Release with `itofin_handle_release`.
/// # Safety
/// `out` holds one writable handle and follows the crate-level contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_convergence_statistics_new(
    ctx: *mut Context,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_error(error)?;
            check_ptr(out)?;
            let id = c.insert(shared_mut(ConvergenceStatistics::new()))?;
            out.write(id);
            Ok(())
        })
    }
}

/// Atomically append one finite observation with finite nonnegative weight.
/// A zero-total-weight checkpoint is rejected without changing state.
/// # Safety
/// Follow the crate-level context and pointer contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_convergence_statistics_add(
    ctx: *mut Context,
    id: u64,
    value: Real,
    weight: Real,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_error(error)?;
            c.get::<SharedMut<ConvergenceStatistics>>(id)?
                .borrow_mut()
                .add_weighted(value, weight)?;
            Ok(())
        })
    }
}

/// Atomically append a bounded batch; null weights with length zero selects unit weights.
/// Invalid values, weights or checkpoints leave all state unchanged.
/// # Safety
/// Inputs follow the crate-level contract and hold their stated lengths.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_convergence_statistics_add_batch(
    ctx: *mut Context,
    id: u64,
    values: *const Real,
    len: usize,
    weights: *const Real,
    weights_len: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_error(error)?;
            let state = c.get::<SharedMut<ConvergenceStatistics>>(id)?;
            let mut state = state.borrow_mut();
            let count = state
                .samples()
                .checked_add(len)
                .ok_or_else(|| BindingError::invalid("convergence sample count overflow"))?;
            validate_convergence_length(count)?;
            let weights = optional_weights(weights, weights_len, len)?;
            state.add_batch(input_slice(values, len)?, weights)?;
            Ok(())
        })
    }
}

/// Remove all observations and checkpoints.
/// # Safety
/// Follow the crate-level context and pointer contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_convergence_statistics_reset(
    ctx: *mut Context,
    id: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_error(error)?;
            c.get::<SharedMut<ConvergenceStatistics>>(id)?
                .borrow_mut()
                .reset();
            Ok(())
        })
    }
}

/// Copy sample count, cumulative weight and completed table length.
/// Outputs are optional but at least one must be present; errors preserve all outputs.
/// # Safety
/// Non-null outputs hold one writable value and follow the crate-level contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_convergence_statistics_summary(
    ctx: *mut Context,
    id: u64,
    out_samples: *mut usize,
    out_weight_sum: *mut Real,
    out_entries: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_error(error)?;
            if out_samples.is_null() && out_weight_sum.is_null() && out_entries.is_null() {
                return Err(BindingError::invalid(
                    "at least one convergence summary output is required",
                ));
            }
            if !out_samples.is_null() {
                check_ptr(out_samples)?;
            }
            if !out_weight_sum.is_null() {
                check_ptr(out_weight_sum)?;
            }
            if !out_entries.is_null() {
                check_ptr(out_entries)?;
            }
            let state = c.get::<SharedMut<ConvergenceStatistics>>(id)?;
            let state = state.borrow();
            if !out_samples.is_null() {
                out_samples.write(state.samples());
            }
            if !out_weight_sum.is_null() {
                out_weight_sum.write(state.weight_sum());
            }
            if !out_entries.is_null() {
                out_entries.write(state.convergence_table().len());
            }
            Ok(())
        })
    }
}

/// Copy the current running weighted mean, including observations after the last checkpoint.
/// An empty accumulator is an error; errors leave `out` unchanged.
/// # Safety
/// `out` holds one writable double and follows the crate-level contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_convergence_statistics_mean(
    ctx: *mut Context,
    id: u64,
    out: *mut Real,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_error(error)?;
            check_ptr(out)?;
            let result = c
                .get::<SharedMut<ConvergenceStatistics>>(id)?
                .borrow()
                .mean()?;
            out.write(result);
            Ok(())
        })
    }
}

/// Copy the completed convergence table into independent caller-owned arrays.
/// Output length must exactly match the completed table; errors preserve both arrays.
/// # Safety
/// Outputs follow the crate-level contract and hold `out_len` writable values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_convergence_statistics_table(
    ctx: *mut Context,
    id: u64,
    out_counts: *mut usize,
    out_means: *mut Real,
    out_len: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_error(error)?;
            let state = c.get::<SharedMut<ConvergenceStatistics>>(id)?;
            let state = state.borrow();
            let points = state.convergence_table();
            check_table_outputs(out_counts, out_means, out_len, points.len())?;
            write_table(points, out_counts, out_means);
            Ok(())
        })
    }
}

#[cfg(test)]
#[path = "convergence_statistics_state_tests.rs"]
mod state_tests;
#[cfg(test)]
#[path = "convergence_statistics_api_tests.rs"]
mod tests;
