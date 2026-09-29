//! Stateless chart indicators with caller-owned output buffers.

use crate::boundary::{
    BindingError, BindingResult, ItofinError, check_ptr, input_slice, without_context,
};
use libitofin::math::chart::{ChartSeries, ema, sma, volume_bars};
use libitofin::types::Real;

/// # Safety
/// Inputs and outputs follow the crate-level pointer and non-overlap contract.
unsafe fn average(
    close: *const Real,
    len: usize,
    period: usize,
    out: *mut Real,
    capacity: usize,
    first_valid: *mut usize,
    compute: fn(&[Real], usize) -> libitofin::errors::QlResult<ChartSeries>,
) -> BindingResult<()> {
    if capacity < len {
        return Err(BindingError::invalid("output capacity too small"));
    }
    check_ptr(first_valid)?;
    if len > 0 {
        check_ptr(out)?;
    }
    let values = compute(unsafe { input_slice(close, len)? }, period)?;
    if len > 0 {
        unsafe { std::ptr::copy_nonoverlapping(values.values.as_ptr(), out, len) };
    }
    unsafe { first_valid.write(values.first_valid) };
    Ok(())
}

/// Compute an input-aligned simple moving average. Prefix values before
/// `first_valid` are zero placeholders, not observations.
/// # Safety
/// Follow the crate-level pointer/non-overlap contract. `out` holds `capacity`
/// doubles and `first_valid` holds one size_t.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_chart_sma(
    close: *const Real,
    len: usize,
    period: usize,
    out: *mut Real,
    capacity: usize,
    first_valid: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            average(close, len, period, out, capacity, first_valid, sma)
        })
    }
}

/// Compute an input-aligned exponential moving average seeded by SMA.
/// # Safety
/// Follow the crate-level pointer/non-overlap contract. `out` holds `capacity`
/// doubles and `first_valid` holds one size_t.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_chart_ema(
    close: *const Real,
    len: usize,
    period: usize,
    out: *mut Real,
    capacity: usize,
    first_valid: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            average(close, len, period, out, capacity, first_valid, ema)
        })
    }
}

/// Copy volume and classify close relative to open as -1, 0, or 1.
/// # Safety
/// Each input has `len` doubles. `out_volume` and `out_direction` each hold
/// `capacity` entries and obey the crate-level non-overlap contract.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_chart_volume_bars(
    open: *const Real,
    high: *const Real,
    low: *const Real,
    close: *const Real,
    volume: *const Real,
    len: usize,
    out_volume: *mut Real,
    out_direction: *mut i8,
    capacity: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            if capacity < len {
                return Err(BindingError::invalid("output capacity too small"));
            }
            if len > 0 {
                check_ptr(out_volume)?;
                check_ptr(out_direction)?;
            }
            let bars = volume_bars(
                input_slice(open, len)?,
                input_slice(high, len)?,
                input_slice(low, len)?,
                input_slice(close, len)?,
                input_slice(volume, len)?,
            )?;
            if len > 0 {
                std::ptr::copy_nonoverlapping(bars.volume.values.as_ptr(), out_volume, len);
                std::ptr::copy_nonoverlapping(bars.direction.as_ptr(), out_direction, len);
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{itofin_chart_ema, itofin_chart_sma, itofin_chart_volume_bars};
    use crate::boundary::ItofinError;

    fn blank_error() -> ItofinError {
        ItofinError {
            code: 0,
            message: [0; 1024],
        }
    }

    #[test]
    fn averages_report_alignment_and_preserve_output_on_error() {
        let close = [1.0, 2.0, 3.0, 4.0];
        let mut values = [9.0; 4];
        let mut first_valid = usize::MAX;
        let mut error = blank_error();
        let code = unsafe {
            itofin_chart_sma(
                close.as_ptr(),
                close.len(),
                3,
                values.as_mut_ptr(),
                values.len(),
                &mut first_valid,
                &mut error,
            )
        };
        assert_eq!(code, 0);
        assert_eq!(first_valid, 2);
        assert_eq!(values, [0.0, 0.0, 2.0, 3.0]);

        values.fill(9.0);
        let code = unsafe {
            itofin_chart_ema(
                close.as_ptr(),
                close.len(),
                0,
                values.as_mut_ptr(),
                values.len(),
                &mut first_valid,
                &mut error,
            )
        };
        assert_ne!(code, 0);
        assert_eq!(values, [9.0; 4]);
        assert_eq!(first_valid, 2);

        let code = unsafe {
            itofin_chart_sma(
                close.as_ptr(),
                close.len(),
                3,
                values.as_mut_ptr(),
                values.len() - 1,
                &mut first_valid,
                &mut error,
            )
        };
        assert_ne!(code, 0);
        assert_eq!(values, [9.0; 4]);
        assert_eq!(first_valid, 2);
    }

    #[test]
    fn volume_bars_copy_both_outputs() {
        let open = [1.0, 2.0, 2.0];
        let high = [3.0; 3];
        let low = [0.0; 3];
        let close = [2.0, 1.0, 2.0];
        let volume = [10.0, 11.0, 12.0];
        let mut out_volume = [0.0; 3];
        let mut direction = [0; 3];
        let mut error = blank_error();
        let code = unsafe {
            itofin_chart_volume_bars(
                open.as_ptr(),
                high.as_ptr(),
                low.as_ptr(),
                close.as_ptr(),
                volume.as_ptr(),
                3,
                out_volume.as_mut_ptr(),
                direction.as_mut_ptr(),
                3,
                &mut error,
            )
        };
        assert_eq!(code, 0);
        assert_eq!(out_volume, volume);
        assert_eq!(direction, [1, -1, 0]);

        let invalid_volume = [10.0, -1.0, 12.0];
        let code = unsafe {
            itofin_chart_volume_bars(
                open.as_ptr(),
                high.as_ptr(),
                low.as_ptr(),
                close.as_ptr(),
                invalid_volume.as_ptr(),
                3,
                out_volume.as_mut_ptr(),
                direction.as_mut_ptr(),
                3,
                &mut error,
            )
        };
        assert_ne!(code, 0);
        assert_eq!(out_volume, volume);
        assert_eq!(direction, [1, -1, 0]);
    }
}
