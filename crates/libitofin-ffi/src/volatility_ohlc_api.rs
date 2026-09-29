//! Pointwise OHLC volatility estimators with caller-owned aligned output.

use crate::boundary::{
    BindingError, BindingResult, ItofinError, check_ptr, input_slice, without_context,
};
use libitofin::math::volatility::{
    OhlcPointEstimates, ohlc_point_volatility, ohlc_point_volatility_constant_fraction,
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
    compute: impl FnOnce() -> BindingResult<OhlcPointEstimates>,
) -> BindingResult<()> {
    let count = len
        .checked_mul(4)
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
        estimates.simple_sigma,
        estimates.parkinson_sigma,
        estimates.garman_klass_sigma4,
        estimates.garman_klass_sigma5,
    ];
    if channels
        .iter()
        .any(|series| series.values.len() != len || series.first_valid != 0)
    {
        return Err(BindingError::invalid(
            "OHLC volatility output alignment mismatch",
        ));
    }
    for (channel, series) in channels.iter().enumerate() {
        if len > 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(series.values.as_ptr(), out.add(channel * len), len)
            };
        }
    }
    unsafe { first_valid.write(0) };
    Ok(())
}

/// Compute SimpleSigma, ParkinsonSigma, Sigma4 and Sigma5 in channel-major
/// order using one positive year fraction per bar. Every bar is valid.
/// # Safety
/// Inputs hold `len` doubles; `out` holds `capacity` doubles and `first_valid`
/// one size_t. Follow the crate-level pointer/non-overlap contract.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_volatility_ohlc_point(
    open: *const Real,
    high: *const Real,
    low: *const Real,
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
            write_estimates(len, out, capacity, first_valid, || {
                let prices = input_prices(open, high, low, close, len)?;
                Ok(ohlc_point_volatility(
                    &prices,
                    input_slice(year_fractions, len)?,
                )?)
            })
        })
    }
}

/// Compute SimpleSigma, ParkinsonSigma, Sigma4 and Sigma5 in channel-major
/// order using one positive year fraction for every bar. Every bar is valid.
/// # Safety
/// Inputs hold `len` doubles; `out` holds `capacity` doubles and `first_valid`
/// one size_t. Follow the crate-level pointer/non-overlap contract.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn itofin_volatility_ohlc_point_constant_fraction(
    open: *const Real,
    high: *const Real,
    low: *const Real,
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
            write_estimates(len, out, capacity, first_valid, || {
                let prices = input_prices(open, high, low, close, len)?;
                Ok(ohlc_point_volatility_constant_fraction(
                    &prices,
                    year_fraction,
                )?)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{itofin_volatility_ohlc_point, itofin_volatility_ohlc_point_constant_fraction};
    use crate::boundary::ItofinError;

    fn blank_error() -> ItofinError {
        ItofinError {
            code: 0,
            message: [0; 1024],
        }
    }

    #[test]
    fn quantlib_fixture_and_channel_order() {
        let open = [100.0, 200.0];
        let high = [110.0, 220.0];
        let low = [90.0, 180.0];
        let close = [105.0, 210.0];
        let fractions = [1.0 / 252.0, 1.0 / 365.0];
        let mut out = [f64::NAN; 8];
        let mut scalar = [f64::NAN; 4];
        let mut first_valid = usize::MAX;
        let mut error = blank_error();
        let code = unsafe {
            itofin_volatility_ohlc_point(
                open.as_ptr(),
                high.as_ptr(),
                low.as_ptr(),
                close.as_ptr(),
                fractions.as_ptr(),
                2,
                out.as_mut_ptr(),
                out.len(),
                &mut first_valid,
                &mut error,
            )
        };
        assert_eq!(code, 0);
        assert_eq!(first_valid, 0);
        let expected = [
            0.7745198449099887,
            1.9131168640323526,
            2.204975405342331,
            2.2004838300550182,
        ];
        for channel in 0..4 {
            assert!((out[channel * 2] - expected[channel]).abs() < 1e-12);
            assert!(
                (out[channel * 2 + 1] - expected[channel] * (365.0_f64 / 252.0).sqrt()).abs()
                    < 1e-12
            );
        }
        let code = unsafe {
            itofin_volatility_ohlc_point_constant_fraction(
                open.as_ptr(),
                high.as_ptr(),
                low.as_ptr(),
                close.as_ptr(),
                1,
                fractions[0],
                scalar.as_mut_ptr(),
                scalar.len(),
                &mut first_valid,
                &mut error,
            )
        };
        assert_eq!(code, 0);
        assert_eq!(first_valid, 0);
        for channel in 0..4 {
            assert!((scalar[channel] - expected[channel]).abs() < 1e-12);
        }
    }

    #[test]
    fn invalid_inputs_preserve_every_output() {
        let open = [100.0];
        let high = [110.0];
        let low = [90.0];
        let close = [105.0];
        let bad_close = [120.0];
        let bad_fraction = [0.0];
        let mut out = [91.0; 4];
        let mut first_valid = 77;
        let mut error = blank_error();
        let valid_fraction = [1.0 / 252.0];
        let cases: [(&[f64], &[f64], usize); 3] = [
            (&bad_close, &valid_fraction, 4),
            (&close, &bad_fraction, 4),
            (&close, &valid_fraction, 3),
        ];
        for (closes, fractions, capacity) in cases {
            let code = unsafe {
                itofin_volatility_ohlc_point(
                    open.as_ptr(),
                    high.as_ptr(),
                    low.as_ptr(),
                    closes.as_ptr(),
                    fractions.as_ptr(),
                    1,
                    out.as_mut_ptr(),
                    capacity,
                    &mut first_valid,
                    &mut error,
                )
            };
            assert_ne!(code, 0);
            assert_eq!(out, [91.0; 4]);
            assert_eq!(first_valid, 77);
        }
        let code = unsafe {
            itofin_volatility_ohlc_point_constant_fraction(
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                0.0,
                std::ptr::null_mut(),
                0,
                &mut first_valid,
                &mut error,
            )
        };
        assert_ne!(code, 0);
        assert_eq!(first_valid, 77);
    }

    #[test]
    fn empty_indexed_input_is_valid() {
        let mut first_valid = usize::MAX;
        let mut error = blank_error();
        let code = unsafe {
            itofin_volatility_ohlc_point(
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                0,
                &mut first_valid,
                &mut error,
            )
        };
        assert_eq!(code, 0);
        assert_eq!(first_valid, 0);
    }
}
