//! Stateless C facade for explicit-frequency performance ratios.

use crate::boundary::{ItofinError, check_ptr, input_slice, without_context};
use libitofin::math::statistics::{sharpe_ratio, sortino_ratio, target_downside_deviation};

/// All-observation RMS shortfall below a per-period target, without annualization.
/// On failure `out` is unchanged. An all-above-target sample returns zero.
///
/// # Safety
/// `returns` holds `len` readable doubles and `out` one writable double.
/// All pointers follow the crate-level alignment and non-overlap contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_target_downside_deviation(
    returns: *const f64,
    len: usize,
    target: f64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            check_ptr(out)?;
            if !error.is_null() {
                check_ptr(error)?;
            }
            let result = target_downside_deviation(input_slice(returns, len)?, target)?;
            out.write(result);
            Ok(())
        })
    }
}

/// Arithmetic Sharpe ratio using scalar per-period risk-free return, sample
/// deviation (`N-1`) and mandatory `sqrt(periods_per_year)` scaling.
/// Requires two finite observations and nonzero dispersion. Failure preserves `out`.
///
/// # Safety
/// `returns` holds `len` readable doubles and `out` one writable double.
/// All pointers follow the crate-level alignment and non-overlap contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_sharpe_ratio(
    returns: *const f64,
    len: usize,
    risk_free_return: f64,
    periods_per_year: f64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            check_ptr(out)?;
            if !error.is_null() {
                check_ptr(error)?;
            }
            let result = sharpe_ratio(
                input_slice(returns, len)?,
                risk_free_return,
                periods_per_year,
            )?;
            out.write(result);
            Ok(())
        })
    }
}

/// Arithmetic Sortino ratio using scalar per-period MAR, all-observation
/// downside (`N`) and mandatory `sqrt(periods_per_year)` scaling.
/// Requires two finite observations and nonzero downside. Failure preserves `out`.
///
/// # Safety
/// `returns` holds `len` readable doubles and `out` one writable double.
/// All pointers follow the crate-level alignment and non-overlap contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_sortino_ratio(
    returns: *const f64,
    len: usize,
    minimum_acceptable_return: f64,
    periods_per_year: f64,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            check_ptr(out)?;
            if !error.is_null() {
                check_ptr(error)?;
            }
            let result = sortino_ratio(
                input_slice(returns, len)?,
                minimum_acceptable_return,
                periods_per_year,
            )?;
            out.write(result);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn performance_ratios_misaligned_error_preserves_output() {
        let values = [-1.0, 2.0];
        let mut output = 99.0;
        let mut storage = [0_u64; 130];
        let error = unsafe {
            storage
                .as_mut_ptr()
                .cast::<u8>()
                .add(1)
                .cast::<ItofinError>()
        };
        unsafe {
            assert_ne!(
                itofin_target_downside_deviation(values.as_ptr(), 2, 0.0, &mut output, error),
                0
            );
            assert_eq!(output, 99.0);
            assert_ne!(
                itofin_sharpe_ratio(values.as_ptr(), 2, 0.0, 12.0, &mut output, error),
                0
            );
            assert_eq!(output, 99.0);
            assert_ne!(
                itofin_sortino_ratio(values.as_ptr(), 2, 0.0, 12.0, &mut output, error),
                0
            );
            assert_eq!(output, 99.0);
        }
    }
}
