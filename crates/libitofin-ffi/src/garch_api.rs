//! GARCH(1,1) filtering and fitting with caller-owned output.

use crate::boundary::{BindingError, ItofinError, check_ptr, input_slice, without_context};
use libitofin::math::garch::Garch11;
use libitofin::types::Real;

/// Fit a stationary GARCH(1,1) model to a return series and forecast one variance.
/// # Safety
/// `returns` holds `len` doubles. All output pointers hold one double and
/// follow the crate-level non-overlap contract.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_garch11_fit(
    returns: *const Real,
    len: usize,
    out_alpha: *mut Real,
    out_beta: *mut Real,
    out_omega: *mut Real,
    out_log_likelihood: *mut Real,
    out_next_variance: *mut Real,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            if len == 0 || len > isize::MAX as usize / size_of::<Real>() {
                return Err(BindingError::invalid("invalid GARCH return length"));
            }
            for pointer in [
                out_alpha,
                out_beta,
                out_omega,
                out_log_likelihood,
                out_next_variance,
            ] {
                check_ptr(pointer)?;
            }
            let input = input_slice(returns, len)?;
            let result = Garch11::fit(input)?;
            out_alpha.write(result.alpha);
            out_beta.write(result.beta);
            out_omega.write(result.omega);
            out_log_likelihood.write(result.log_likelihood);
            out_next_variance.write(result.next_variance);
            Ok(())
        })
    }
}

/// Filter returns into conditional volatility and forecast the next variance.
/// The first output slot is a zero warmup placeholder; `first_valid` is one.
/// # Safety
/// `returns` holds `len` doubles and `out` holds `capacity` doubles. All
/// pointers follow the crate-level non-overlap contract.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_garch11_filter(
    returns: *const Real,
    len: usize,
    alpha: Real,
    beta: Real,
    long_run_variance: Real,
    out: *mut Real,
    capacity: usize,
    first_valid: *mut usize,
    next_variance: *mut Real,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            if len == 0 || len > isize::MAX as usize / size_of::<Real>() {
                return Err(BindingError::invalid("invalid GARCH return length"));
            }
            if capacity < len {
                return Err(BindingError::invalid("output capacity too small"));
            }
            check_ptr(out)?;
            check_ptr(first_valid)?;
            check_ptr(next_variance)?;
            let input = input_slice(returns, len)?;
            let result = Garch11::new(alpha, beta, long_run_variance)?.filter(input)?;
            if result.conditional_volatility.values.len() != len
                || result.conditional_volatility.first_valid != 1
            {
                return Err(BindingError::invalid("GARCH output alignment mismatch"));
            }
            std::ptr::copy_nonoverlapping(result.conditional_volatility.values.as_ptr(), out, len);
            first_valid.write(1);
            next_variance.write(result.next_variance);
            Ok(())
        })
    }
}

/// Forecast one variance from the latest return and current variance.
/// # Safety
/// `out_variance` is writable and follows the crate-level pointer contract.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_garch11_forecast(
    last_return: Real,
    current_variance: Real,
    alpha: Real,
    beta: Real,
    long_run_variance: Real,
    out_variance: *mut Real,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            check_ptr(out_variance)?;
            let variance = Garch11::new(alpha, beta, long_run_variance)?
                .forecast(last_return, current_variance)?;
            out_variance.write(variance);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{itofin_garch11_filter, itofin_garch11_fit, itofin_garch11_forecast};
    use crate::boundary::ItofinError;

    fn blank_error() -> ItofinError {
        ItofinError {
            code: 0,
            message: [0; 1024],
        }
    }

    #[test]
    fn fitted_result_and_error_preserve_outputs() {
        let returns: Vec<f64> = (0..320)
            .map(|index| {
                let t = index as f64;
                (0.3 * (t * 2.41).sin() + 0.2 * (t * 0.39).cos()) * (1.0 + 0.4 * (t * 0.09).sin())
            })
            .collect();
        let mut values = [f64::NAN; 5];
        let mut error = blank_error();
        let output = values.as_mut_ptr();
        let status = unsafe {
            itofin_garch11_fit(
                returns.as_ptr(),
                returns.len(),
                output.add(0),
                output.add(1),
                output.add(2),
                output.add(3),
                output.add(4),
                &mut error,
            )
        };
        assert_eq!(status, 0);
        assert!(values.iter().all(|value| value.is_finite()));
        assert!(values[0] >= 0.0 && values[1] >= 0.0 && values[0] + values[1] < 1.0);
        assert!(values[2] > 0.0 && values[4] > 0.0);

        values = [91.0; 5];
        let output = values.as_mut_ptr();
        let status = unsafe {
            itofin_garch11_fit(
                returns.as_ptr(),
                1,
                output.add(0),
                output.add(1),
                output.add(2),
                output.add(3),
                output.add(4),
                &mut error,
            )
        };
        assert_ne!(status, 0);
        assert_eq!(values, [91.0; 5]);
    }

    #[test]
    fn quantlib_fitted_oracle() {
        let bytes = include_bytes!("../../libitofin/tests/fixtures/garch_fit/returns.bin");
        let returns: Vec<f64> = bytes
            .chunks_exact(8)
            .map(|chunk| {
                let mut word = [0_u8; 8];
                word.copy_from_slice(chunk);
                f64::from_le_bytes(word)
            })
            .collect();
        assert_eq!(returns.len(), 50_000);
        let mut values = [f64::NAN; 5];
        let output = values.as_mut_ptr();
        let mut error = blank_error();
        let status = unsafe {
            itofin_garch11_fit(
                returns.as_ptr(),
                returns.len(),
                output.add(0),
                output.add(1),
                output.add(2),
                output.add(3),
                output.add(4),
                &mut error,
            )
        };
        assert_eq!(status, 0);
        for (actual, expected) in values.into_iter().zip([
            0.207_591_659_556_347_29,
            0.281_978_985_012_917_74,
            0.204_647_052_450_554_6,
            -0.021_741_348_447_339_62,
            0.593_706_642_894_903_7,
        ]) {
            assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
        }
    }

    #[test]
    fn quantlib_fixture_and_scalar_forecast() {
        let returns = [0.1; 10];
        let mut output = [f64::NAN; 10];
        let mut first_valid = usize::MAX;
        let mut next_variance = f64::NAN;
        let mut error = blank_error();
        let status = unsafe {
            itofin_garch11_filter(
                returns.as_ptr(),
                returns.len(),
                0.2,
                0.3,
                0.4,
                output.as_mut_ptr(),
                output.len(),
                &mut first_valid,
                &mut next_variance,
                &mut error,
            )
        };
        assert_eq!(status, 0);
        assert_eq!(first_valid, 1);
        assert_eq!(output[0], 0.0);
        let expected = [
            0.452769, 0.513323, 0.530141, 0.5350841, 0.536558, 0.536999, 0.537132, 0.537171,
            0.537183, 0.537187,
        ];
        for (actual, expected) in output[1..].iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-6);
        }
        assert!((next_variance.sqrt() - expected[9]).abs() < 1e-6);
        let mut scalar = f64::NAN;
        let status = unsafe {
            itofin_garch11_forecast(
                0.1,
                output[9].powi(2),
                0.2,
                0.3,
                0.4,
                &mut scalar,
                &mut error,
            )
        };
        assert_eq!(status, 0);
        assert_eq!(scalar, next_variance);
    }

    #[test]
    fn one_return_and_errors_leave_outputs_untouched() {
        let value = [0.1];
        let mut output = [91.0];
        let mut first_valid = 77;
        let mut next_variance = 88.0;
        let mut error = blank_error();
        let status = unsafe {
            itofin_garch11_filter(
                value.as_ptr(),
                1,
                0.2,
                0.3,
                0.4,
                output.as_mut_ptr(),
                1,
                &mut first_valid,
                &mut next_variance,
                &mut error,
            )
        };
        assert_eq!(status, 0);
        assert_eq!(output, [0.0]);
        assert_eq!(first_valid, 1);
        assert!((next_variance - 0.205).abs() < 1e-15);

        output = [91.0];
        first_valid = 77;
        next_variance = 88.0;
        for (len, alpha, capacity) in [(0, 0.2, 1), (1, -0.2, 1), (1, 0.2, 0)] {
            let status = unsafe {
                itofin_garch11_filter(
                    value.as_ptr(),
                    len,
                    alpha,
                    0.3,
                    0.4,
                    output.as_mut_ptr(),
                    capacity,
                    &mut first_valid,
                    &mut next_variance,
                    &mut error,
                )
            };
            assert_ne!(status, 0);
            assert_eq!(output, [91.0]);
            assert_eq!(first_valid, 77);
            assert_eq!(next_variance, 88.0);
        }
        let huge = [f64::MAX];
        let status = unsafe {
            itofin_garch11_filter(
                huge.as_ptr(),
                1,
                0.2,
                0.3,
                0.4,
                output.as_mut_ptr(),
                1,
                &mut first_valid,
                &mut next_variance,
                &mut error,
            )
        };
        assert_ne!(status, 0);
        assert_eq!(output, [91.0]);
        assert_eq!(first_valid, 77);
        assert_eq!(next_variance, 88.0);
        let status = unsafe {
            itofin_garch11_forecast(f64::MAX, 0.4, 0.2, 0.3, 0.4, &mut next_variance, &mut error)
        };
        assert_ne!(status, 0);
        assert_eq!(next_variance, 88.0);
    }
}
