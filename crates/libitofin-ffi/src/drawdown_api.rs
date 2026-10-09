//! Stateless maximum drawdown with an atomic caller-owned result.

use crate::boundary::{ItofinError, check_ptr, input_slice, without_context};
use libitofin::math::statistics::maximum_drawdown;
use libitofin::types::Real;

/// Maximum fractional running-peak loss and zero-based peak/trough indices.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItofinDrawdownResult {
    /// Nonnegative fractional loss, not a percentage or signed return.
    pub drawdown: Real,
    /// Earliest equal running peak before the winning trough.
    pub peak_index: usize,
    /// First trough with the greatest computed loss.
    pub trough_index: usize,
}

/// Evaluate ordered finite strictly positive NAVs. Empty input is an error;
/// one NAV or no decline returns zero and indices (0, 0). Tied peaks keep the
/// earliest index and tied losses keep the first trough. Errors leave `out`
/// unchanged. The result owns no handles or resources and needs no destruction.
/// # Safety
/// `values` holds `len` readable doubles and `out` one writable result. Follow
/// the crate-level alignment, lifetime and non-overlap pointer contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_maximum_drawdown(
    values: *const Real,
    len: usize,
    out: *mut ItofinDrawdownResult,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let result = maximum_drawdown(input_slice(values, len)?)?;
            out.write(ItofinDrawdownResult {
                drawdown: result.drawdown,
                peak_index: result.peak_index,
                trough_index: result.trough_index,
            });
            Ok(())
        })
    }
}

#[cfg(test)]
#[path = "drawdown_api_tests.rs"]
mod tests;
