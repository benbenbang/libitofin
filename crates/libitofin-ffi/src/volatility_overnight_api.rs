//! Overnight OHLC volatility estimators with caller-owned aligned output.

use crate::boundary::{
    BindingError, BindingResult, ItofinError, check_ptr, input_slice, without_context,
};
use libitofin::math::volatility::{
    OhlcOvernightEstimates, ohlc_overnight_volatility, ohlc_overnight_volatility_constant_fraction,
};
use libitofin::prices::IntervalPrice;
use libitofin::types::Real;

/// # Safety
/// Each input holds `len` doubles and follows the crate-level pointer contract.
unsafe fn input_prices(
    open: *const Real,
    high: *const Real,
    low: *const Real,
    close: *const Real,
    len: usize,
) -> BindingResult<Vec<IntervalPrice>> {
    let opens = unsafe { input_slice(open, len)? };
    let highs = unsafe { input_slice(high, len)? };
    let lows = unsafe { input_slice(low, len)? };
    let closes = unsafe { input_slice(close, len)? };
    let mut prices = Vec::new();
    prices
        .try_reserve_exact(len)
        .map_err(|_| BindingError::invalid("OHLC input allocation failed"))?;
    for index in 0..len {
        prices.push(IntervalPrice::new(
            opens[index],
            highs[index],
            lows[index],
            closes[index],
        )?);
    }
    Ok(prices)
}

/// # Safety
/// `out` holds `capacity` doubles; `first_valid` holds one size_t. All pointers
/// follow the crate-level non-overlap contract.
unsafe fn write_estimates(
    len: usize,
    out: *mut Real,
    capacity: usize,
    first_valid: *mut usize,
    compute: impl FnOnce() -> BindingResult<OhlcOvernightEstimates>,
) -> BindingResult<()> {
    let count = len
        .checked_mul(3)
        .filter(|count| *count <= isize::MAX as usize / size_of::<Real>())
        .ok_or_else(|| BindingError::invalid("OHLC output size overflow"))?;
    if capacity < count {
        return Err(BindingError::invalid("output capacity too small"));
    }
    check_ptr(first_valid)?;
    if count > 0 {
        check_ptr(out)?;
    }
    let estimates = compute()?;
    let channels = [
        estimates.garman_klass_sigma1,
        estimates.garman_klass_sigma3,
        estimates.garman_klass_sigma6,
    ];
    let expected_first_valid = len.min(1);
    if channels
        .iter()
        .any(|series| series.values.len() != len || series.first_valid != expected_first_valid)
    {
        return Err(BindingError::invalid(
            "OHLC overnight volatility output alignment mismatch",
        ));
    }
    for (channel, series) in channels.iter().enumerate() {
        if len > 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(series.values.as_ptr(), out.add(channel * len), len)
            };
        }
    }
    unsafe { first_valid.write(expected_first_valid) };
    Ok(())
}

/// Compute Sigma1, Sigma3, and Sigma6 using each interval's year fraction.
/// Index zero is a missing-prefix placeholder and its year fraction is unused.
/// # Safety
/// Inputs hold `len` doubles; `out` holds `capacity` doubles and `first_valid`
/// one size_t. Follow the crate-level pointer/non-overlap contract.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_volatility_ohlc_overnight(
    open: *const Real,
    high: *const Real,
    low: *const Real,
    close: *const Real,
    year_fractions: *const Real,
    len: usize,
    overnight_fraction: Real,
    out: *mut Real,
    capacity: usize,
    first_valid: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            write_estimates(len, out, capacity, first_valid, || {
                let prices = input_prices(open, high, low, close, len)?;
                Ok(ohlc_overnight_volatility(
                    &prices,
                    input_slice(year_fractions, len)?,
                    overnight_fraction,
                )?)
            })
        })
    }
}

/// Compute Sigma1, Sigma3, and Sigma6 with one common year fraction.
/// Index zero is a missing-prefix placeholder.
/// # Safety
/// Inputs hold `len` doubles; `out` holds `capacity` doubles and `first_valid`
/// one size_t. Follow the crate-level pointer/non-overlap contract.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_volatility_ohlc_overnight_constant_fraction(
    open: *const Real,
    high: *const Real,
    low: *const Real,
    close: *const Real,
    len: usize,
    year_fraction: Real,
    overnight_fraction: Real,
    out: *mut Real,
    capacity: usize,
    first_valid: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            write_estimates(len, out, capacity, first_valid, || {
                let prices = input_prices(open, high, low, close, len)?;
                Ok(ohlc_overnight_volatility_constant_fraction(
                    &prices,
                    year_fraction,
                    overnight_fraction,
                )?)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        itofin_volatility_ohlc_overnight, itofin_volatility_ohlc_overnight_constant_fraction,
    };
    use crate::boundary::ItofinError;

    fn blank_error() -> ItofinError {
        ItofinError {
            code: 0,
            message: [0; 1024],
        }
    }

    #[test]
    fn quantlib_fixture_channel_order_and_scalar_parity() {
        let open = [100.0, 110.0];
        let high = [100.0, 120.0];
        let low = [100.0, 105.0];
        let close = [100.0, 115.0];
        let fractions = [f64::NAN, 1.0 / 252.0];
        let mut indexed = [f64::NAN; 6];
        let mut scalar = [f64::NAN; 6];
        let mut first_valid = usize::MAX;
        let mut error = blank_error();
        let code = unsafe {
            itofin_volatility_ohlc_overnight(
                open.as_ptr(),
                high.as_ptr(),
                low.as_ptr(),
                close.as_ptr(),
                fractions.as_ptr(),
                2,
                0.25,
                indexed.as_mut_ptr(),
                indexed.len(),
                &mut first_valid,
                &mut error,
            )
        };
        assert_eq!(code, 0);
        assert_eq!(first_valid, 1);
        let expected = [2.2159224836472786, 1.8303355611439194, 1.6795672145926133];
        for (channel, value) in expected.into_iter().enumerate() {
            assert_eq!(indexed[channel * 2], 0.0);
            assert!((indexed[channel * 2 + 1] - value).abs() < 1e-12);
        }
        let code = unsafe {
            itofin_volatility_ohlc_overnight_constant_fraction(
                open.as_ptr(),
                high.as_ptr(),
                low.as_ptr(),
                close.as_ptr(),
                2,
                1.0 / 252.0,
                0.25,
                scalar.as_mut_ptr(),
                scalar.len(),
                &mut first_valid,
                &mut error,
            )
        };
        assert_eq!(code, 0);
        assert_eq!(indexed, scalar);
        assert_eq!(first_valid, 1);
    }

    #[test]
    fn errors_preserve_caller_output() {
        let open = [100.0, 110.0];
        let high = [100.0, 120.0];
        let low = [100.0, 105.0];
        let close = [100.0, 115.0];
        let bad_close = [100.0, 125.0];
        let fractions = [0.0, 1.0 / 252.0];
        let mut out = [91.0; 6];
        let mut first_valid = 77;
        let mut error = blank_error();
        for (closes, fraction, capacity) in [
            (&bad_close[..], 0.25, 6),
            (&close[..], 0.0, 6),
            (&close[..], 0.25, 5),
        ] {
            let code = unsafe {
                itofin_volatility_ohlc_overnight(
                    open.as_ptr(),
                    high.as_ptr(),
                    low.as_ptr(),
                    closes.as_ptr(),
                    fractions.as_ptr(),
                    2,
                    fraction,
                    out.as_mut_ptr(),
                    capacity,
                    &mut first_valid,
                    &mut error,
                )
            };
            assert_ne!(code, 0);
            assert_eq!(out, [91.0; 6]);
            assert_eq!(first_valid, 77);
        }
    }

    #[test]
    fn empty_and_one_bar_keep_missing_prefix() {
        let mut first_valid = usize::MAX;
        let mut error = blank_error();
        let code = unsafe {
            itofin_volatility_ohlc_overnight(
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                0.25,
                std::ptr::null_mut(),
                0,
                &mut first_valid,
                &mut error,
            )
        };
        assert_eq!(code, 0);
        assert_eq!(first_valid, 0);
        let value = [100.0];
        let fraction = [f64::NAN];
        let mut out = [f64::NAN; 3];
        let code = unsafe {
            itofin_volatility_ohlc_overnight(
                value.as_ptr(),
                value.as_ptr(),
                value.as_ptr(),
                value.as_ptr(),
                fraction.as_ptr(),
                1,
                0.25,
                out.as_mut_ptr(),
                out.len(),
                &mut first_valid,
                &mut error,
            )
        };
        assert_eq!(code, 0);
        assert_eq!(first_valid, 1);
        assert_eq!(out, [0.0; 3]);
    }
}
