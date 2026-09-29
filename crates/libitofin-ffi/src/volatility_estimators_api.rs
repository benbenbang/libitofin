//! Close-price volatility estimators with aligned caller-owned output.

use crate::boundary::{
    BindingError, BindingResult, ItofinError, check_ptr, input_slice, without_context,
};
use libitofin::math::chart::ChartSeries;
use libitofin::math::volatility::{
    constant_volatility, simple_local_volatility, simple_local_volatility_constant_fraction,
};
use libitofin::types::Real;

/// # Safety
/// `out` holds `capacity` doubles and `first_valid` one size_t. All pointers
/// follow the crate-level non-overlap contract.
unsafe fn write_estimate(
    len: usize,
    out: *mut Real,
    capacity: usize,
    first_valid: *mut usize,
    compute: impl FnOnce() -> BindingResult<ChartSeries>,
) -> BindingResult<()> {
    if capacity < len {
        return Err(BindingError::invalid("output capacity too small"));
    }
    check_ptr(first_valid)?;
    if len > 0 {
        check_ptr(out)?;
    }
    let result = compute()?;
    if result.values.len() != len {
        return Err(BindingError::invalid("volatility output length mismatch"));
    }
    if len > 0 {
        unsafe { std::ptr::copy_nonoverlapping(result.values.as_ptr(), out, len) };
    }
    unsafe { first_valid.write(result.first_valid) };
    Ok(())
}

/// Estimate annualized local volatility from closes and per-bar year fractions.
/// The fraction at index zero is unused. `first_valid` is one when closes are
/// present; earlier output slots are zero warmup placeholders.
/// # Safety
/// `close` and `year_fractions` hold `len` doubles; output pointers follow the
/// crate-level non-overlap contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_volatility_simple_local(
    close: *const Real,
    year_fractions: *const Real,
    len: usize,
    out: *mut Real,
    capacity: usize,
    first_valid: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            write_estimate(len, out, capacity, first_valid, || {
                Ok(simple_local_volatility(
                    input_slice(close, len)?,
                    input_slice(year_fractions, len)?,
                )?)
            })
        })
    }
}

/// Estimate local volatility using a single year fraction for every interval.
/// # Safety
/// `close` holds `len` doubles; output pointers follow the crate-level
/// non-overlap contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_volatility_simple_local_constant_fraction(
    close: *const Real,
    len: usize,
    year_fraction: Real,
    out: *mut Real,
    capacity: usize,
    first_valid: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            write_estimate(len, out, capacity, first_valid, || {
                Ok(simple_local_volatility_constant_fraction(
                    input_slice(close, len)?,
                    year_fraction,
                )?)
            })
        })
    }
}

/// Compose a volatility series from the preceding `window` valid inputs.
/// `input_first_valid` identifies the first real input; earlier values are
/// warmup placeholders and never enter a window.
/// # Safety
/// `values` holds `len` doubles; output pointers follow the crate-level
/// non-overlap contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_volatility_constant(
    values: *const Real,
    len: usize,
    input_first_valid: usize,
    window: usize,
    out: *mut Real,
    capacity: usize,
    first_valid: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            write_estimate(len, out, capacity, first_valid, || {
                if input_first_valid > len {
                    return Err(BindingError::invalid(
                        "input first-valid index exceeds length",
                    ));
                }
                if window == 0 {
                    return Err(BindingError::invalid("volatility window must be positive"));
                }
                let input = input_slice(values, len)?;
                let mut owned = Vec::new();
                owned
                    .try_reserve_exact(len)
                    .map_err(|_| BindingError::invalid("volatility input allocation failed"))?;
                owned.extend_from_slice(input);
                Ok(constant_volatility(
                    &ChartSeries {
                        values: owned,
                        first_valid: input_first_valid,
                    },
                    window,
                )?)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        itofin_volatility_constant, itofin_volatility_simple_local,
        itofin_volatility_simple_local_constant_fraction,
    };
    use crate::boundary::ItofinError;

    fn blank_error() -> ItofinError {
        ItofinError {
            code: 0,
            message: [0; 1024],
        }
    }

    #[test]
    fn quantlib_close_estimators_compose_without_current_bar() {
        let close = [100.0, 110.0, 99.0];
        let fractions = [0.0, 1.0 / 252.0, 1.0 / 252.0];
        let mut local = [f64::NAN; 3];
        let mut scalar = [f64::NAN; 3];
        let mut constant = [f64::NAN; 3];
        let mut first_valid = usize::MAX;
        let mut error = blank_error();
        let code = unsafe {
            itofin_volatility_simple_local(
                close.as_ptr(),
                fractions.as_ptr(),
                close.len(),
                local.as_mut_ptr(),
                local.len(),
                &mut first_valid,
                &mut error,
            )
        };
        assert_eq!(code, 0);
        assert_eq!(first_valid, 1);
        assert_eq!(local[0], 0.0);
        assert!((local[1] - 1.513_002_199_050_567_3).abs() < 1e-12);
        assert!((local[2] - 1.672_546_334_616_811_2).abs() < 1e-12);
        let code = unsafe {
            itofin_volatility_simple_local_constant_fraction(
                close.as_ptr(),
                close.len(),
                1.0 / 252.0,
                scalar.as_mut_ptr(),
                scalar.len(),
                &mut first_valid,
                &mut error,
            )
        };
        assert_eq!(code, 0);
        assert_eq!(scalar, local);
        let code = unsafe {
            itofin_volatility_constant(
                local.as_ptr(),
                local.len(),
                1,
                1,
                constant.as_mut_ptr(),
                constant.len(),
                &mut first_valid,
                &mut error,
            )
        };
        assert_eq!(code, 0);
        assert_eq!(first_valid, 2);
        assert_eq!(&constant[..2], &[0.0, 0.0]);
        assert!((constant[2] - 1.069_854_114_898_814_5).abs() < 1e-12);
    }

    #[test]
    fn invalid_inputs_leave_outputs_untouched() {
        let close = [100.0, 0.0];
        let fractions = [0.0, 1.0 / 252.0];
        let mut out = [91.0, 92.0];
        let mut first_valid = 77;
        let mut error = blank_error();
        let code = unsafe {
            itofin_volatility_simple_local(
                close.as_ptr(),
                fractions.as_ptr(),
                2,
                out.as_mut_ptr(),
                out.len(),
                &mut first_valid,
                &mut error,
            )
        };
        assert_ne!(code, 0);
        assert_eq!(out, [91.0, 92.0]);
        assert_eq!(first_valid, 77);
        let code = unsafe {
            itofin_volatility_constant(
                fractions.as_ptr(),
                2,
                3,
                1,
                out.as_mut_ptr(),
                out.len(),
                &mut first_valid,
                &mut error,
            )
        };
        assert_ne!(code, 0);
        assert_eq!(out, [91.0, 92.0]);
        assert_eq!(first_valid, 77);
        let code = unsafe {
            itofin_volatility_constant(
                fractions.as_ptr(),
                2,
                1,
                0,
                out.as_mut_ptr(),
                out.len(),
                &mut first_valid,
                &mut error,
            )
        };
        assert_ne!(code, 0);
        assert_eq!(out, [91.0, 92.0]);
        assert_eq!(first_valid, 77);
        let valid = [100.0, 110.0];
        let bad_fractions = [0.0, 0.0];
        let code = unsafe {
            itofin_volatility_simple_local(
                valid.as_ptr(),
                bad_fractions.as_ptr(),
                2,
                out.as_mut_ptr(),
                out.len(),
                &mut first_valid,
                &mut error,
            )
        };
        assert_ne!(code, 0);
        assert_eq!(out, [91.0, 92.0]);
        assert_eq!(first_valid, 77);
        let code = unsafe {
            itofin_volatility_simple_local(
                valid.as_ptr(),
                fractions.as_ptr(),
                2,
                out.as_mut_ptr(),
                1,
                &mut first_valid,
                &mut error,
            )
        };
        assert_ne!(code, 0);
        assert_eq!(out, [91.0, 92.0]);
        assert_eq!(first_valid, 77);
    }
}
