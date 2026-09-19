use super::*;
use crate::calendar_api::NativeCalendar;
use libitofin::quotes::SimpleQuote;
use libitofin::settings::Settings;
use libitofin::time::{
    calendars::target::Target,
    date::{Date, Month},
    daycounters::actual365fixed::Actual365Fixed,
};
use std::ptr::{null, null_mut};

#[test]
fn additional_matrix_exports_validate_buffers_modes_and_dates() {
    let mut c = Context::new();
    let today = Date::new(15, Month::June, 2026);
    let calendar = c
        .insert(NativeCalendar {
            inner: Target::new(),
            first_year: 1901,
            horizon: None,
        })
        .unwrap();
    let day_counter = c.insert(Actual365Fixed::new()).unwrap();
    let setting = shared(Settings::new());
    setting.set_evaluation_date(today);
    let settings = c.insert(setting).unwrap();
    let quote = c.insert(shared(SimpleQuote::new(0.13))).unwrap();
    let lengths = [1, 6];
    let units = [2, 2];
    let swap_lengths = [1, 5];
    let swap_units = [3, 3];
    let values = [0.13, 0.15, 0.14, 0.16];
    let quotes = [quote; 4];
    let dates = [(today + 30).serial_number(), (today + 180).serial_number()];
    let mut cfg = ItofinVolGridConfig {
        reference_date: today.serial_number(),
        settlement_days: 0,
        calendar,
        convention: 1,
        day_counter,
        settings: 0,
        option_lengths: lengths.as_ptr(),
        option_units: units.as_ptr(),
        rows: 2,
        swap_lengths: swap_lengths.as_ptr(),
        swap_units: swap_units.as_ptr(),
        strikes: null(),
        columns: 2,
        values: values.as_ptr(),
        quotes: quotes.as_ptr(),
        count: 4,
        shifts: null(),
        shift_count: 0,
        volatility_type: 0,
        flat_extrapolation: 0,
    };
    let mut out = 0;
    let mut error = ItofinError {
        code: 0,
        message: [0; 1024],
    };
    unsafe {
        for build in [
            itofin_swaption_vol_matrix_fixed_quotes,
            itofin_swaption_vol_matrix_moving_matrix,
        ] {
            assert_eq!(
                build(&mut c, null(), &mut out, &mut error),
                INVALID_ARGUMENT
            );
            assert_eq!(
                build(&mut c, &cfg, null_mut(), &mut error),
                INVALID_ARGUMENT
            );
        }
        assert_eq!(
            itofin_swaption_vol_matrix_fixed_quotes(&mut c, &cfg, &mut out, &mut error),
            0
        );
        cfg.quotes = null();
        assert_eq!(
            itofin_swaption_vol_matrix_fixed_quotes(&mut c, &cfg, &mut out, &mut error),
            INVALID_ARGUMENT
        );
        cfg.settings = settings;
        assert_eq!(
            itofin_swaption_vol_matrix_moving_matrix(&mut c, &cfg, &mut out, &mut error),
            0
        );
        assert_eq!(
            itofin_swaption_vol_matrix_fixed_quotes(&mut c, &cfg, &mut out, &mut error),
            INVALID_ARGUMENT
        );
        cfg.values = null();
        assert_eq!(
            itofin_swaption_vol_matrix_moving_matrix(&mut c, &cfg, &mut out, &mut error),
            INVALID_ARGUMENT
        );
        cfg.values = values.as_ptr();
        cfg.settings = 0;
        assert_eq!(
            itofin_swaption_vol_matrix_dates(&mut c, &cfg, dates.as_ptr(), 2, &mut out, &mut error),
            0
        );
        assert_eq!(
            itofin_swaption_vol_matrix_dates(&mut c, &cfg, null(), 2, &mut out, &mut error),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_swaption_vol_matrix_dates(&mut c, &cfg, dates.as_ptr(), 1, &mut out, &mut error),
            INVALID_ARGUMENT
        );
        cfg.flat_extrapolation = 2;
        assert_eq!(
            itofin_swaption_vol_matrix_dates(&mut c, &cfg, dates.as_ptr(), 2, &mut out, &mut error),
            INVALID_ARGUMENT
        );
        cfg.flat_extrapolation = 0;
        cfg.shift_count = 1;
        assert_eq!(
            itofin_swaption_vol_matrix_dates(&mut c, &cfg, dates.as_ptr(), 2, &mut out, &mut error),
            INVALID_ARGUMENT
        );
    }
}
