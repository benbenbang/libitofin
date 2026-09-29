//! Dated OHLC prices with caller-owned arrays and no native handles.

use crate::boundary::{BindingError, ItofinError, check_ptr, input_slice, without_context};
use crate::time_api::date;
use libitofin::prices::PriceSeries;
use libitofin::types::Real;

/// One dated OHLC observation. Dates are QuantLib-compatible serial numbers.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItofinDatedIntervalPrice {
    pub date: i32,
    pub open: Real,
    pub high: Real,
    pub low: Real,
    pub close: Real,
}

/// Validate dated OHLC prices, sort by date, and keep the last input for a
/// repeated date. `capacity` must hold at least `len` rows. Neither output is
/// changed on error.
/// # Safety
/// Follow the crate-level pointer/non-overlap contract. `input` holds `len`
/// rows, `out` holds `capacity` rows, and `out_len` holds one size_t.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_interval_prices_normalize(
    input: *const ItofinDatedIntervalPrice,
    len: usize,
    out: *mut ItofinDatedIntervalPrice,
    capacity: usize,
    out_len: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        without_context(error, || {
            if len > isize::MAX as usize / size_of::<ItofinDatedIntervalPrice>() {
                return Err(BindingError::invalid("price series length overflow"));
            }
            if capacity < len {
                return Err(BindingError::invalid("output capacity too small"));
            }
            check_ptr(out_len)?;
            if len > 0 {
                check_ptr(out)?;
            }
            let rows = input_slice(input, len)?;
            let mut dates = Vec::new();
            let mut opens = Vec::new();
            let mut highs = Vec::new();
            let mut lows = Vec::new();
            let mut closes = Vec::new();
            dates
                .try_reserve_exact(len)
                .map_err(|_| BindingError::invalid("price series allocation failed"))?;
            opens
                .try_reserve_exact(len)
                .map_err(|_| BindingError::invalid("price series allocation failed"))?;
            highs
                .try_reserve_exact(len)
                .map_err(|_| BindingError::invalid("price series allocation failed"))?;
            lows.try_reserve_exact(len)
                .map_err(|_| BindingError::invalid("price series allocation failed"))?;
            closes
                .try_reserve_exact(len)
                .map_err(|_| BindingError::invalid("price series allocation failed"))?;
            for row in rows {
                dates.push(date(row.date)?);
                opens.push(row.open);
                highs.push(row.high);
                lows.push(row.low);
                closes.push(row.close);
            }
            let series = PriceSeries::from_slices(&dates, &opens, &highs, &lows, &closes)?;
            for (index, (when, price)) in series.iter().enumerate() {
                out.add(index).write(ItofinDatedIntervalPrice {
                    date: when.serial_number(),
                    open: price.open(),
                    high: price.high(),
                    low: price.low(),
                    close: price.close(),
                });
            }
            out_len.write(series.len());
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ItofinDatedIntervalPrice, itofin_interval_prices_normalize};
    use crate::boundary::ItofinError;

    fn blank_error() -> ItofinError {
        ItofinError {
            code: 0,
            message: [0; 1024],
        }
    }

    fn row(date: i32, close: f64) -> ItofinDatedIntervalPrice {
        ItofinDatedIntervalPrice {
            date,
            open: close,
            high: close + 1.0,
            low: close - 1.0,
            close,
        }
    }

    #[test]
    fn sorts_and_overwrites_duplicates_without_touching_input() {
        let input = [row(45_002, 2.0), row(45_000, -2.0), row(45_002, 3.0)];
        let original = input;
        let mut out = [row(45_001, 99.0); 3];
        let mut out_len = usize::MAX;
        let mut error = blank_error();
        let code = unsafe {
            itofin_interval_prices_normalize(
                input.as_ptr(),
                input.len(),
                out.as_mut_ptr(),
                out.len(),
                &mut out_len,
                &mut error,
            )
        };
        assert_eq!(code, 0);
        assert_eq!(out_len, 2);
        assert_eq!(out[0], row(45_000, -2.0));
        assert_eq!(out[1], row(45_002, 3.0));
        assert_eq!(input, original);

        let code = unsafe {
            itofin_interval_prices_normalize(
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                0,
                &mut out_len,
                &mut error,
            )
        };
        assert_eq!(code, 0);
        assert_eq!(out_len, 0);
    }

    #[test]
    fn rejects_invalid_inputs_without_partial_outputs() {
        let mut out = [row(45_001, 99.0); 2];
        let sentinel = out;
        let mut out_len = usize::MAX;
        let mut error = blank_error();
        for input in [
            row(0, 2.0),
            row(45_000, f64::NAN),
            ItofinDatedIntervalPrice {
                high: 0.0,
                ..row(45_000, 2.0)
            },
        ] {
            let code = unsafe {
                itofin_interval_prices_normalize(
                    &input,
                    1,
                    out.as_mut_ptr(),
                    out.len(),
                    &mut out_len,
                    &mut error,
                )
            };
            assert_ne!(code, 0);
            assert_eq!(out, sentinel);
            assert_eq!(out_len, usize::MAX);
        }
        let valid = row(45_000, 2.0);
        let code = unsafe {
            itofin_interval_prices_normalize(
                &valid,
                1,
                out.as_mut_ptr(),
                0,
                &mut out_len,
                &mut error,
            )
        };
        assert_ne!(code, 0);
        assert_eq!(out, sentinel);
        assert_eq!(out_len, usize::MAX);
        let code = unsafe {
            itofin_interval_prices_normalize(
                &valid,
                usize::MAX,
                out.as_mut_ptr(),
                usize::MAX,
                &mut out_len,
                &mut error,
            )
        };
        assert_ne!(code, 0);
        assert_eq!(out, sentinel);
        assert_eq!(out_len, usize::MAX);
    }
}
