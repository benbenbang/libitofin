//! Stateful bounded-memory weighted statistics.

use crate::boundary::{
    BindingError, Context, ItofinError, check_ptr, input_slice, output, with_context,
};
use libitofin::math::statistics::{IncrementalStatistics, MeanStdDev, Statistics};
use libitofin::shared::{SharedMut, shared_mut};
use libitofin::types::Real;

fn sample(value: Real, weight: Real) -> Result<(), BindingError> {
    if !value.is_finite() || !weight.is_finite() || weight < 0.0 {
        return Err(BindingError::invalid(
            "statistics values and nonnegative weights must be finite",
        ));
    }
    Ok(())
}

fn checked_result(value: Real) -> Result<Real, BindingError> {
    if !value.is_finite() {
        return Err(BindingError::invalid("statistics result is nonfinite"));
    }
    Ok(value)
}

/// Create an empty incremental accumulator in the caller's context.
///
/// # Safety
/// Follow the crate-level C caller contract; `out` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_incremental_statistics_new(
    ctx: *mut Context,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            output(out, c.insert(shared_mut(IncrementalStatistics::new()))?)
        })
    }
}

/// Add one signed observation with its nonnegative weight.
///
/// # Safety
/// Follow the crate-level C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_incremental_statistics_add(
    ctx: *mut Context,
    id: u64,
    value: Real,
    weight: Real,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            sample(value, weight)?;
            let state = c.get::<SharedMut<IncrementalStatistics>>(id)?;
            let mut next = state.borrow().clone();
            next.add_weighted(value, weight)?;
            if !next.has_finite_state() {
                return Err(BindingError::invalid(
                    "statistics accumulator state is nonfinite",
                ));
            }
            *state.borrow_mut() = next;
            Ok(())
        })
    }
}

/// Atomically add a batch; null weights and zero weight length mean unit weights.
///
/// # Safety
/// `values` and optional `weights` hold readable `len` doubles.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_incremental_statistics_add_batch(
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
            if len == 0 {
                return Err(BindingError::invalid("statistics batch must not be empty"));
            }
            let weights = if weights.is_null() && weights_len == 0 {
                None
            } else {
                if weights_len != len {
                    return Err(BindingError::invalid(
                        "observation and weight lengths differ",
                    ));
                }
                Some(input_slice(weights, weights_len)?)
            };
            let values = input_slice(values, len)?;
            for (index, &value) in values.iter().enumerate() {
                sample(value, weights.map_or(1.0, |weights| weights[index]))?;
            }
            let state = c.get::<SharedMut<IncrementalStatistics>>(id)?;
            let mut next = state.borrow().clone();
            for (index, &value) in values.iter().enumerate() {
                next.add_weighted(value, weights.map_or(1.0, |weights| weights[index]))?;
                if !next.has_finite_state() {
                    return Err(BindingError::invalid(
                        "statistics accumulator state is nonfinite",
                    ));
                }
            }
            *state.borrow_mut() = next;
            Ok(())
        })
    }
}

/// Reset the accumulator without releasing its handle.
///
/// # Safety
/// Follow the crate-level C caller contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_incremental_statistics_reset(
    ctx: *mut Context,
    id: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            c.get::<SharedMut<IncrementalStatistics>>(id)?
                .borrow_mut()
                .reset();
            Ok(())
        })
    }
}

/// Read the total sample count (0) or negative sample count (1).
///
/// # Safety
/// `out` is writable and does not overlap context or error storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_incremental_statistics_count(
    ctx: *mut Context,
    id: u64,
    which: i32,
    out: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let state = c.get::<SharedMut<IncrementalStatistics>>(id)?;
            let state = state.borrow();
            let value = match which {
                0 => state.samples(),
                1 => state.downside_samples(),
                _ => return Err(BindingError::invalid("unknown incremental count")),
            };
            output(out, value)
        })
    }
}

/// Read a scalar statistic: weight sum (0), downside weight sum (1),
/// min (2), max (3), mean (4), variance (5), standard deviation (6),
/// error estimate (7), skewness (8), kurtosis (9), downside variance (10),
/// or downside deviation (11).
///
/// # Safety
/// `out` is writable and does not overlap context or error storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_incremental_statistics_query(
    ctx: *mut Context,
    id: u64,
    measure: i32,
    out: *mut Real,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let state = c.get::<SharedMut<IncrementalStatistics>>(id)?;
            let state = state.borrow();
            let value = match measure {
                0 => state.weight_sum(),
                1 => state.downside_weight_sum(),
                2 => state.min()?,
                3 => state.max()?,
                4 => state.mean()?,
                5 => state.variance()?,
                6 => state.standard_deviation()?,
                7 => state.error_estimate()?,
                8 => state.skewness()?,
                9 => state.kurtosis()?,
                10 => state.downside_variance()?,
                11 => state.downside_deviation()?,
                _ => return Err(BindingError::invalid("unknown incremental statistic")),
            };
            output(out, checked_result(value)?)
        })
    }
}

#[cfg(test)]
#[path = "incremental_statistics_api_tests.rs"]
mod tests;
