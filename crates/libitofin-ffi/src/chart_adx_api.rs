//! Caller-owned buffers for Wilder directional indicators.

use crate::boundary::{BindingError, ItofinError, check_ptr, input_slice, without_context};
use libitofin::math::chart::adx;
use libitofin::types::Real;

/// Compute Wilder +DI, -DI, DX and ADX in channel-major order.
/// Each channel holds `len` values, with zero warmup placeholders. Four
/// corresponding validity indices are written. DI/DX start at `period`;
/// ADX starts at `2*period-1`, capped at `len`. Period one is supported.
/// Inputs and arithmetic are validated before any output is written.
///
/// # Safety
/// Follow the crate-level pointer/non-overlap contract. Each HLC input holds
/// `len` doubles. `out` holds `capacity` doubles. `first_valid` holds
/// `first_valid_capacity` size_t entries, with capacity at least four.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_chart_adx(
    high: *const Real,
    low: *const Real,
    close: *const Real,
    len: usize,
    period: usize,
    out: *mut Real,
    capacity: usize,
    first_valid: *mut usize,
    first_valid_capacity: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            if !error.is_null() {
                check_ptr(error)?;
            }
            let count = len
                .checked_mul(4)
                .filter(|count| *count <= isize::MAX as usize / size_of::<Real>())
                .ok_or_else(|| BindingError::invalid("chart output size overflow"))?;
            if capacity < count || first_valid_capacity < 4 {
                return Err(BindingError::invalid("output capacity too small"));
            }
            check_ptr(first_valid)?;
            if count > 0 {
                check_ptr(out)?;
            }
            let result = adx(
                input_slice(high, len)?,
                input_slice(low, len)?,
                input_slice(close, len)?,
                period,
            )?;
            for (channel, series) in [&result.plus_di, &result.minus_di, &result.dx, &result.adx]
                .into_iter()
                .enumerate()
            {
                if len > 0 {
                    std::ptr::copy_nonoverlapping(
                        series.values.as_ptr(),
                        out.add(channel * len),
                        len,
                    );
                }
                first_valid.add(channel).write(series.first_valid);
            }
            Ok(())
        })
    }
}

#[cfg(test)]
#[path = "chart_adx_tests.rs"]
mod tests;
