//! Stateless empirical statistics over signed, optionally weighted observations.

use crate::boundary::{BindingError, ItofinError, check_ptr, input_slice, without_context};
use libitofin::math::statistics::{BatchStatistic, evaluate_batch};
use libitofin::types::Real;

/// Evaluate one weighted batch statistic into caller-owned storage.
///
/// `measure` selects mean (0), sample variance (1), standard deviation (2),
/// percentile (3), value at risk (4), or expected shortfall (5). `probability`
/// is used for percentile and risk measures only. Null `weights` with zero
/// `weights_len` selects unit weights. Inputs are signed observations; VaR and
/// expected shortfall return nonnegative loss magnitudes. On error, `out` is
/// unchanged.
///
/// # Safety
///
/// `values` holds `values_len` readable doubles. If supplied, `weights` holds
/// `weights_len` readable doubles. `out` holds one writable double. All
/// pointers follow the crate-level alignment and non-overlap contract.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_statistics_evaluate(
    values: *const Real,
    values_len: usize,
    weights: *const Real,
    weights_len: usize,
    measure: i32,
    probability: Real,
    out: *mut Real,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            check_ptr(out)?;
            let measure = match measure {
                0 => BatchStatistic::Mean,
                1 => BatchStatistic::Variance,
                2 => BatchStatistic::StandardDeviation,
                3 => BatchStatistic::Percentile,
                4 => BatchStatistic::ValueAtRisk,
                5 => BatchStatistic::ExpectedShortfall,
                _ => return Err(BindingError::invalid("unknown batch statistic measure")),
            };
            let weights = if weights.is_null() && weights_len == 0 {
                None
            } else {
                if weights_len != values_len {
                    return Err(BindingError::invalid(
                        "observation and weight lengths differ",
                    ));
                }
                Some(input_slice(weights, weights_len)?)
            };
            let values = input_slice(values, values_len)?;
            let result = evaluate_batch(values, weights, measure, probability)?;
            out.write(result);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boundary::{CORE_ERROR, INVALID_ARGUMENT};

    fn blank_error() -> ItofinError {
        ItofinError {
            code: 0,
            message: [0; 1024],
        }
    }

    #[test]
    fn weighted_results_and_unit_weights_match_core() {
        let values = [-4.0, -2.0, 2.0, 8.0];
        let weights = [1.0, 2.0, 1.0, 0.0];
        let mut out = Real::NAN;
        let mut error = blank_error();
        for (measure, probability, expected) in [
            (0, 0.0, -1.5),
            (1, 0.0, 19.0 / 3.0),
            (2, 0.0, (19.0_f64 / 3.0).sqrt()),
            (3, 0.75, -2.0),
            (4, 0.9, 4.0),
        ] {
            let status = unsafe {
                itofin_statistics_evaluate(
                    values.as_ptr(),
                    values.len(),
                    weights.as_ptr(),
                    weights.len(),
                    measure,
                    probability,
                    &mut out,
                    &mut error,
                )
            };
            assert_eq!(status, 0);
            assert_eq!(error.code, 0);
            assert_eq!(out, expected);
        }
        let values = [-8.0, -4.0, -2.0, 2.0];
        let weights = [0.1, 2.0, 1.0, 1.0];
        let status = unsafe {
            itofin_statistics_evaluate(
                values.as_ptr(),
                values.len(),
                weights.as_ptr(),
                weights.len(),
                5,
                0.9,
                &mut out,
                &mut error,
            )
        };
        assert_eq!(status, 0);
        assert_eq!(out, 8.0);
        let status = unsafe {
            itofin_statistics_evaluate(
                values.as_ptr(),
                values.len(),
                std::ptr::null(),
                0,
                0,
                0.0,
                &mut out,
                &mut error,
            )
        };
        assert_eq!(status, 0);
        assert_eq!(out, -3.0);
    }

    #[test]
    fn every_error_preserves_output() {
        let values = [-4.0, 2.0];
        let bad_weights = [0.0, 0.0];
        let mut out = 91.0;
        let mut error = blank_error();
        let cases = [
            (values.as_ptr(), 0, std::ptr::null(), 0, 0, 0.0),
            (std::ptr::null(), 2, std::ptr::null(), 0, 0, 0.0),
            (values.as_ptr(), 2, std::ptr::null(), 2, 0, 0.0),
            (values.as_ptr(), 2, bad_weights.as_ptr(), 1, 0, 0.0),
            (values.as_ptr(), 2, bad_weights.as_ptr(), 2, 0, 0.0),
            (values.as_ptr(), 2, std::ptr::null(), 0, 3, Real::NAN),
            (values.as_ptr(), 2, std::ptr::null(), 0, 4, 1.0),
            (values.as_ptr(), 2, std::ptr::null(), 0, 5, 0.9),
            (values.as_ptr(), 2, std::ptr::null(), 0, 6, 0.0),
        ];
        for (pointer, len, weights, weights_len, measure, probability) in cases {
            let status = unsafe {
                itofin_statistics_evaluate(
                    pointer,
                    len,
                    weights,
                    weights_len,
                    measure,
                    probability,
                    &mut out,
                    &mut error,
                )
            };
            assert!(status == CORE_ERROR || status == INVALID_ARGUMENT);
            assert_eq!(error.code, status);
            assert_eq!(out, 91.0);
        }
        let status = unsafe {
            itofin_statistics_evaluate(
                values.as_ptr(),
                values.len(),
                std::ptr::null(),
                0,
                0,
                0.0,
                std::ptr::null_mut(),
                &mut error,
            )
        };
        assert_eq!(status, INVALID_ARGUMENT);
    }
}
