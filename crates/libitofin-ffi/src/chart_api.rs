//! Stateless chart indicators with caller-owned output buffers.

use crate::boundary::{
    BindingError, BindingResult, ItofinError, check_ptr, input_slice, without_context,
};
use libitofin::math::chart::{
    ChartSeries, bollinger_bands, ema, kd, macd, obv, rsi, sma, volume_bars, vwap,
};
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

/// # Safety
/// Inputs and outputs follow the crate-level pointer and non-overlap contract.
unsafe fn volume_line(
    price: *const Real,
    volume: *const Real,
    len: usize,
    out: *mut Real,
    capacity: usize,
    first_valid: *mut usize,
    compute: fn(&[Real], &[Real]) -> libitofin::errors::QlResult<ChartSeries>,
) -> BindingResult<()> {
    if capacity < len {
        return Err(BindingError::invalid("output capacity too small"));
    }
    check_ptr(first_valid)?;
    if len > 0 {
        check_ptr(out)?;
    }
    let result = compute(unsafe { input_slice(price, len)? }, unsafe {
        input_slice(volume, len)?
    })?;
    if len > 0 {
        unsafe { std::ptr::copy_nonoverlapping(result.values.as_ptr(), out, len) };
    }
    unsafe { first_valid.write(result.first_valid) };
    Ok(())
}

/// Compute cumulative VWAP of supplied prices, starting a session per call.
/// Zero cumulative volume is missing; later zero volume carries the last VWAP.
/// # Safety
/// Inputs each hold `len` doubles. Follow the crate-level pointer/non-overlap
/// contract. `out` holds `capacity` doubles and `first_valid` holds one size_t.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_chart_vwap(
    price: *const Real,
    volume: *const Real,
    len: usize,
    out: *mut Real,
    capacity: usize,
    first_valid: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            volume_line(price, volume, len, out, capacity, first_valid, vwap)
        })
    }
}

/// Compute zero-seeded OBV; equal closes leave signed volume unchanged.
/// The initial volume is validated but does not contribute to the seed.
/// # Safety
/// Inputs each hold `len` doubles. Follow the crate-level pointer/non-overlap
/// contract. `out` holds `capacity` doubles and `first_valid` holds one size_t.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_chart_obv(
    close: *const Real,
    volume: *const Real,
    len: usize,
    out: *mut Real,
    capacity: usize,
    first_valid: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            volume_line(close, volume, len, out, capacity, first_valid, obv)
        })
    }
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

/// Compute population-standard-deviation Bollinger bands. Output is channel
/// major: middle, upper, then lower, with `len` values per channel.
/// # Safety
/// Follow the crate-level pointer/non-overlap contract. `out` holds
/// `capacity` doubles and `first_valid` holds one size_t.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_chart_bollinger(
    close: *const Real,
    len: usize,
    period: usize,
    multiplier: Real,
    out: *mut Real,
    capacity: usize,
    first_valid: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            let count = len
                .checked_mul(3)
                .filter(|count| *count <= isize::MAX as usize / size_of::<Real>())
                .ok_or_else(|| BindingError::invalid("chart output size overflow"))?;
            if capacity < count {
                return Err(BindingError::invalid("output capacity too small"));
            }
            check_ptr(first_valid)?;
            if count > 0 {
                check_ptr(out)?;
            }
            let bands = bollinger_bands(input_slice(close, len)?, period, multiplier)?;
            if len > 0 {
                std::ptr::copy_nonoverlapping(bands.middle.values.as_ptr(), out, len);
                std::ptr::copy_nonoverlapping(bands.upper.values.as_ptr(), out.add(len), len);
                std::ptr::copy_nonoverlapping(bands.lower.values.as_ptr(), out.add(len * 2), len);
            }
            first_valid.write(bands.middle.first_valid);
            Ok(())
        })
    }
}

/// Compute Wilder RSI from closing prices, aligned to input bars.
/// # Safety
/// Follow the crate-level pointer/non-overlap contract. `out` holds
/// `capacity` doubles and `first_valid` holds one size_t.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_chart_rsi(
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
            average(close, len, period, out, capacity, first_valid, rsi)
        })
    }
}

/// Compute Taiwan KD. Output is channel major: RSV, K, then D, with `len`
/// values per channel. All channels share the returned first-valid index.
/// # Safety
/// Follow the crate-level pointer/non-overlap contract. `out` holds
/// `capacity` doubles and `first_valid` holds one size_t.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_chart_kd(
    high: *const Real,
    low: *const Real,
    close: *const Real,
    len: usize,
    period: usize,
    k_smooth: usize,
    d_smooth: usize,
    out: *mut Real,
    capacity: usize,
    first_valid: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            let count = len
                .checked_mul(3)
                .filter(|count| *count <= isize::MAX as usize / size_of::<Real>())
                .ok_or_else(|| BindingError::invalid("chart output size overflow"))?;
            if capacity < count {
                return Err(BindingError::invalid("output capacity too small"));
            }
            check_ptr(first_valid)?;
            if count > 0 {
                check_ptr(out)?;
            }
            let result = kd(
                input_slice(high, len)?,
                input_slice(low, len)?,
                input_slice(close, len)?,
                period,
                k_smooth,
                d_smooth,
            )?;
            if len > 0 {
                std::ptr::copy_nonoverlapping(result.rsv.values.as_ptr(), out, len);
                std::ptr::copy_nonoverlapping(result.k.values.as_ptr(), out.add(len), len);
                std::ptr::copy_nonoverlapping(result.d.values.as_ptr(), out.add(len * 2), len);
            }
            first_valid.write(result.rsv.first_valid);
            Ok(())
        })
    }
}

/// Compute SMA-seeded MACD. Output is channel major: line, signal, then
/// histogram. `first_valid` receives three corresponding size_t indices.
/// # Safety
/// Follow the crate-level pointer/non-overlap contract. `out` holds
/// `capacity` doubles and `first_valid` holds `first_valid_capacity` size_t.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_chart_macd(
    close: *const Real,
    len: usize,
    fast_period: usize,
    slow_period: usize,
    signal_period: usize,
    out: *mut Real,
    capacity: usize,
    first_valid: *mut usize,
    first_valid_capacity: usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            let count = len
                .checked_mul(3)
                .filter(|count| *count <= isize::MAX as usize / size_of::<Real>())
                .ok_or_else(|| BindingError::invalid("chart output size overflow"))?;
            if capacity < count || first_valid_capacity < 3 {
                return Err(BindingError::invalid("output capacity too small"));
            }
            check_ptr(first_valid)?;
            if count > 0 {
                check_ptr(out)?;
            }
            let result = macd(
                input_slice(close, len)?,
                fast_period,
                slow_period,
                signal_period,
            )?;
            if len > 0 {
                std::ptr::copy_nonoverlapping(result.line.values.as_ptr(), out, len);
                std::ptr::copy_nonoverlapping(result.signal.values.as_ptr(), out.add(len), len);
                std::ptr::copy_nonoverlapping(
                    result.histogram.values.as_ptr(),
                    out.add(len * 2),
                    len,
                );
            }
            first_valid.write(result.line.first_valid);
            first_valid.add(1).write(result.signal.first_valid);
            first_valid.add(2).write(result.histogram.first_valid);
            Ok(())
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
    use super::{
        itofin_chart_bollinger, itofin_chart_ema, itofin_chart_kd, itofin_chart_macd,
        itofin_chart_rsi, itofin_chart_sma, itofin_chart_volume_bars,
    };
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

    #[test]
    fn bollinger_and_rsi_copy_aligned_results() {
        let close = [1.0, 2.0, 3.0, 4.0];
        let mut bands = [9.0; 12];
        let mut first_valid = usize::MAX;
        let mut error = blank_error();
        let code = unsafe {
            itofin_chart_bollinger(
                close.as_ptr(),
                close.len(),
                3,
                2.0,
                bands.as_mut_ptr(),
                bands.len(),
                &mut first_valid,
                &mut error,
            )
        };
        assert_eq!(code, 0);
        assert_eq!(first_valid, 2);
        assert_eq!(&bands[..4], &[0.0, 0.0, 2.0, 3.0]);
        assert!((bands[6] - (2.0 + 2.0 * (2.0_f64 / 3.0).sqrt())).abs() < 1e-12);
        assert!((bands[10] - (2.0 - 2.0 * (2.0_f64 / 3.0).sqrt())).abs() < 1e-12);

        let mut output = [9.0; 4];
        let code = unsafe {
            itofin_chart_rsi(
                [1.0, 2.0, 3.0, 2.0].as_ptr(),
                4,
                2,
                output.as_mut_ptr(),
                output.len(),
                &mut first_valid,
                &mut error,
            )
        };
        assert_eq!(code, 0);
        assert_eq!(first_valid, 2);
        assert_eq!(output, [0.0, 0.0, 100.0, 50.0]);

        output.fill(9.0);
        let code = unsafe {
            itofin_chart_rsi(
                close.as_ptr(),
                close.len(),
                0,
                output.as_mut_ptr(),
                output.len(),
                &mut first_valid,
                &mut error,
            )
        };
        assert_ne!(code, 0);
        assert_eq!(output, [9.0; 4]);
        assert_eq!(first_valid, 2);
    }

    #[test]
    fn kd_and_macd_channels_and_warmup() {
        let high = [12.0, 14.0, 16.0, 18.0];
        let low = [8.0, 10.0, 12.0, 14.0];
        let close = [10.0, 12.0, 14.0, 16.0];
        let mut output = [9.0; 12];
        let mut first_valid = usize::MAX;
        let mut error = blank_error();
        let code = unsafe {
            itofin_chart_kd(
                high.as_ptr(),
                low.as_ptr(),
                close.as_ptr(),
                close.len(),
                3,
                3,
                3,
                output.as_mut_ptr(),
                output.len(),
                &mut first_valid,
                &mut error,
            )
        };
        assert_eq!(code, 0);
        assert_eq!(first_valid, 2);
        assert_eq!(&output[..4], &[0.0, 0.0, 75.0, 75.0]);
        assert!((output[6] - 58.333333333333336).abs() < 1e-12);
        assert!((output[10] - 52.77777777777778).abs() < 1e-12);

        let close = [1.0, 2.0, 3.0, 4.0];
        let mut validity = [usize::MAX; 3];
        let code = unsafe {
            itofin_chart_macd(
                close.as_ptr(),
                close.len(),
                2,
                3,
                2,
                output.as_mut_ptr(),
                output.len(),
                validity.as_mut_ptr(),
                validity.len(),
                &mut error,
            )
        };
        assert_eq!(code, 0);
        assert_eq!(validity, [2, 3, 3]);
        assert_eq!(&output[..4], &[0.0, 0.0, 0.5, 0.5]);
        assert_eq!(&output[4..8], &[0.0, 0.0, 0.0, 0.5]);
        assert_eq!(&output[8..], &[0.0; 4]);

        output.fill(9.0);
        let code = unsafe {
            itofin_chart_macd(
                close.as_ptr(),
                close.len(),
                2,
                3,
                2,
                output.as_mut_ptr(),
                output.len(),
                validity.as_mut_ptr(),
                2,
                &mut error,
            )
        };
        assert_ne!(code, 0);
        assert_eq!(output, [9.0; 12]);
        assert_eq!(validity, [2, 3, 3]);
    }
}

#[cfg(test)]
#[path = "chart_vwap_obv_tests.rs"]
mod vwap_obv_tests;
