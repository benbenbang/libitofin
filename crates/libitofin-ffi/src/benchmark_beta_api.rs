//! Stateless benchmark beta with atomic caller-owned output.

use crate::boundary::{BindingError, ItofinError, check_ptr, input_slice, without_context};
use libitofin::math::statistics::{MAX_SEQUENCE_ROWS, benchmark_beta};
use libitofin::types::Real;

/// Compute asset/benchmark beta from already aligned return samples.
///
/// Requires 2-100,000 observations and equal asset/benchmark lengths. Null
/// weights with zero length selects unit weights; otherwise weights must have
/// one finite nonnegative entry per observation and a finite positive total.
/// Count-corrected covariance/benchmark variance uses all rows, including
/// zero-weight rows. No annualization or risk-free adjustment is applied.
/// Zero benchmark variance and nonfinite calculations fail. On error `out`
/// is unchanged. No handles or retained native allocations are created.
///
/// # Safety
/// Each input points to its stated number of readable doubles and `out` to
/// one writable double. Pointers follow crate-level alignment/non-overlap rules.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_benchmark_beta(
    asset_returns: *const Real,
    asset_len: usize,
    benchmark_returns: *const Real,
    benchmark_len: usize,
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
            check_ptr(out)?;
            if !(2..=MAX_SEQUENCE_ROWS).contains(&asset_len) || asset_len != benchmark_len {
                return Err(BindingError::invalid(
                    "benchmark beta requires 2-100000 aligned rows",
                ));
            }
            let weights = if weights.is_null() && weights_len == 0 {
                None
            } else {
                if weights_len != asset_len {
                    return Err(BindingError::invalid(
                        "benchmark beta weight length mismatch",
                    ));
                }
                Some(input_slice(weights, weights_len)?)
            };
            let asset = input_slice(asset_returns, asset_len)?;
            let benchmark = input_slice(benchmark_returns, benchmark_len)?;
            let result = benchmark_beta(asset, benchmark, weights)?;
            out.write(result);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn misaligned_error_preserves_output() {
        let a = [1.0, 2.0];
        let mut out = 91.0;
        let mut storage = [0_u32; 258];
        let error = unsafe {
            storage
                .as_mut_ptr()
                .cast::<u8>()
                .add(1)
                .cast::<ItofinError>()
        };
        let status = unsafe {
            itofin_benchmark_beta(
                a.as_ptr(),
                2,
                a.as_ptr(),
                2,
                std::ptr::null(),
                0,
                &mut out,
                error,
            )
        };
        assert_eq!(status, crate::boundary::INVALID_ARGUMENT);
        assert_eq!(out, 91.0);
        assert!(storage.iter().all(|&v| v == 0));
    }

    #[test]
    fn benchmark_beta_atomic_and_repeatable() {
        let a = [1.0, 3.0, 2.0, 10.0];
        let b = [-1.0, 0.0, 2.0, 9.0];
        let w = [1.0, 2.0, 3.0, 0.0];
        let mut out = 91.0;
        let mut error = ItofinError {
            code: 0,
            message: [0; 1024],
        };
        for _ in 0..10 {
            let status = unsafe {
                itofin_benchmark_beta(
                    a.as_ptr(),
                    4,
                    b.as_ptr(),
                    4,
                    w.as_ptr(),
                    4,
                    &mut out,
                    &mut error,
                )
            };
            assert_eq!(status, 0);
            assert_eq!(error.code, 0);
            assert!((out - 1.0 / 53.0).abs() < 2e-12);
        }
        for (ap, an, bp, bn, wp, wn) in [
            (std::ptr::null(), 4, b.as_ptr(), 4, w.as_ptr(), 4),
            (a.as_ptr(), 4, std::ptr::null(), 4, w.as_ptr(), 4),
            (a.as_ptr(), 4, b.as_ptr(), 3, w.as_ptr(), 4),
            (a.as_ptr(), 4, b.as_ptr(), 4, std::ptr::null(), 4),
            (a.as_ptr(), 4, b.as_ptr(), 4, w.as_ptr(), 3),
            (a.as_ptr(), 1, b.as_ptr(), 1, std::ptr::null(), 0),
        ] {
            out = 91.0;
            let status =
                unsafe { itofin_benchmark_beta(ap, an, bp, bn, wp, wn, &mut out, &mut error) };
            assert_ne!(status, 0);
            assert_eq!(error.code, status);
            assert_eq!(out, 91.0);
        }
        assert_ne!(
            unsafe {
                itofin_benchmark_beta(
                    a.as_ptr(),
                    4,
                    b.as_ptr(),
                    4,
                    w.as_ptr(),
                    4,
                    std::ptr::null_mut(),
                    &mut error,
                )
            },
            0
        );
        assert_eq!(
            unsafe {
                itofin_benchmark_beta(
                    a.as_ptr(),
                    4,
                    b.as_ptr(),
                    4,
                    std::ptr::null(),
                    0,
                    &mut out,
                    &mut error,
                )
            },
            0
        );
        assert!((out - 53.0 / 61.0).abs() < 2e-12);
    }
}
