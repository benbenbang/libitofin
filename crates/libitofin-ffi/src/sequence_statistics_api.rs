//! Stateless statistics over rectangular, optionally weighted vector samples.

use crate::boundary::{BindingError, ItofinError, check_ptr, input_slice, without_context};
use libitofin::math::statistics::{
    SequenceStatistic, evaluate_sequence_batch, validate_sequence_shape,
};
use libitofin::types::Real;

/// Evaluate weighted vector statistics into a caller-owned row-major buffer.
///
/// `values` holds `rows * dimension` row-major doubles; optional weights have
/// one entry per row. Null weights with zero length selects unit weights.
/// `measure` selects mean (0), variance (1), standard deviation (2), error
/// estimate (3), minimum (4), maximum (5), covariance (6), or correlation (7).
/// The exact output length is `dimension` for 0-5 and `dimension * dimension`
/// for 6-7. Variance/covariance use the observation-count N/(N-1) correction,
/// counting zero-weight rows. On every error, the entire output is unchanged.
/// Dimensions are limited to 1-256, rows to 1-100,000, inputs to 1,000,000
/// values and covariance/correlation work to 100,000,000 row-coordinate pairs.
///
/// # Safety
///
/// Inputs must be readable and `out` writable for their stated lengths.
/// All pointers follow the crate-level alignment and non-overlap contract.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_sequence_statistics_evaluate(
    values: *const Real,
    values_len: usize,
    rows: usize,
    dimension: usize,
    weights: *const Real,
    weights_len: usize,
    measure: i32,
    out: *mut Real,
    out_len: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            if !error.is_null() {
                check_ptr(error)?;
            }
            let measure = match measure {
                0 => SequenceStatistic::Mean,
                1 => SequenceStatistic::Variance,
                2 => SequenceStatistic::StandardDeviation,
                3 => SequenceStatistic::ErrorEstimate,
                4 => SequenceStatistic::Minimum,
                5 => SequenceStatistic::Maximum,
                6 => SequenceStatistic::Covariance,
                7 => SequenceStatistic::Correlation,
                _ => return Err(BindingError::invalid("unknown sequence statistic measure")),
            };
            let expected = validate_sequence_shape(rows, dimension, measure)?;
            let count = rows.checked_mul(dimension).ok_or_else(|| {
                BindingError::invalid("sequence statistics input length overflow")
            })?;
            if values_len != count {
                return Err(BindingError::invalid(
                    "sequence statistics input length mismatch",
                ));
            }
            if out_len != expected {
                return Err(BindingError::invalid(
                    "sequence statistics output length mismatch",
                ));
            }
            check_ptr(out)?;
            let weights = if weights.is_null() && weights_len == 0 {
                None
            } else {
                if weights_len != rows {
                    return Err(BindingError::invalid(
                        "sequence statistics weight length mismatch",
                    ));
                }
                Some(input_slice(weights, weights_len)?)
            };
            let values = input_slice(values, values_len)?;
            let result = evaluate_sequence_batch(values, rows, dimension, weights, measure)?;
            if result.len() != expected {
                return Err(BindingError::invalid(
                    "sequence statistics result length mismatch",
                ));
            }
            std::ptr::copy_nonoverlapping(result.as_ptr(), out, expected);
            Ok(())
        })
    }
}

#[cfg(test)]
#[path = "sequence_statistics_api_tests.rs"]
mod tests;
