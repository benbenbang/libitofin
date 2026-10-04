//! Bounded unit-weight L2 star discrepancy over rectangular unit-cube samples.

use crate::boundary::{BindingError, ItofinError, check_ptr, input_slice, without_context};
use libitofin::math::statistics::{evaluate_discrepancy_batch, validate_discrepancy_shape};
use libitofin::types::Real;

/// Evaluate normalized L2 star discrepancy for row-major points in [0, 1]^d.
/// Dimensions are 2-256, rows 1-4096, inputs at most 1,000,000 coordinates,
/// and pair-coordinate work at most 100,000,000. Null weights with length zero
/// selects unit weights; explicit weights must all equal one. Other weights
/// are rejected. Errors leave `out` unchanged. No random draws are performed.
/// # Safety
/// Inputs are readable for their lengths, `out` holds one writable double,
/// and all pointers follow the crate-level alignment and non-overlap contract.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_discrepancy_statistics_evaluate(
    values: *const Real,
    values_len: usize,
    rows: usize,
    dimension: usize,
    weights: *const Real,
    weights_len: usize,
    out: *mut Real,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            if !error.is_null() {
                check_ptr(error)?;
            }
            validate_discrepancy_shape(rows, dimension)?;
            let count = rows
                .checked_mul(dimension)
                .ok_or_else(|| BindingError::invalid("discrepancy input length overflow"))?;
            if values_len != count {
                return Err(BindingError::invalid("discrepancy input length mismatch"));
            }
            check_ptr(out)?;
            let weights = if weights.is_null() && weights_len == 0 {
                None
            } else {
                if weights_len != rows {
                    return Err(BindingError::invalid(
                        "discrepancy row and weight lengths differ",
                    ));
                }
                Some(input_slice(weights, weights_len)?)
            };
            let result = evaluate_discrepancy_batch(
                input_slice(values, values_len)?,
                rows,
                dimension,
                weights,
            )?;
            out.write(result);
            Ok(())
        })
    }
}

#[cfg(test)]
#[path = "discrepancy_statistics_api_tests.rs"]
mod tests;
