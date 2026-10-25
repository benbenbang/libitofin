use super::*;
use crate::boundary::itofin_handle_release;
use crate::fd_engine_api::{ItofinFdConfig, itofin_fd_black_scholes_engine_new};
use crate::market_api::itofin_black_scholes_new;
use crate::time_api::itofin_day_counter_new;
use libitofin::time::date::Month;
use std::ptr::null_mut;

fn build(
    c: &mut Context,
    settings: u64,
    kind: i32,
    moneyness: f64,
    expiry: i32,
    out: &mut u64,
) -> i32 {
    unsafe {
        itofin_option_percentage_strike_new(c, kind, moneyness, expiry, settings, out, null_mut())
    }
}

#[test]
fn percentage_strike_factory_guards_preserve_output_and_context() {
    let mut c = Context::new();
    let mut other = Context::new();
    let settings = c.insert(shared(Settings::<Date>::new())).unwrap();
    let today = Date::new(15, Month::June, 2026).serial_number();
    let expiry = today + 360;
    let mut out = 123;
    for strike in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            build(&mut c, settings, 0, strike, expiry, &mut out),
            INVALID_ARGUMENT
        );
        assert_eq!(out, 123);
    }
    assert_eq!(
        build(&mut c, settings, 2, 100.0, expiry, &mut out),
        INVALID_ARGUMENT
    );
    assert_eq!(
        build(&mut c, settings, 0, 100.0, 0, &mut out),
        INVALID_ARGUMENT
    );
    assert_eq!(build(&mut c, 0, 0, 100.0, expiry, &mut out), INVALID_HANDLE);
    assert_eq!(
        build(&mut other, settings, 0, 100.0, expiry, &mut out),
        INVALID_HANDLE
    );
    assert_eq!(out, 123);
    unsafe {
        assert_eq!(
            itofin_option_percentage_strike_new(
                &mut c,
                0,
                100.0,
                expiry,
                settings,
                null_mut(),
                null_mut()
            ),
            INVALID_ARGUMENT
        );
    }
    let mut error_storage = [0_u64; 130];
    let bad_error = error_storage
        .as_mut_ptr()
        .cast::<u8>()
        .wrapping_add(1)
        .cast();
    let create = itofin_option_percentage_strike_new;
    let code = unsafe { create(&mut c, 0, 100.0, expiry, settings, &mut out, bad_error) };
    assert_eq!(code, INVALID_ARGUMENT);
    assert_eq!(out, 123);
    let pointer = (&mut c as *mut Context) as usize;
    std::thread::scope(|scope| {
        let (code, untouched) = scope
            .spawn(move || {
                let mut output = 123;
                let code = unsafe {
                    itofin_option_percentage_strike_new(
                        pointer as *mut Context,
                        0,
                        100.0,
                        expiry,
                        settings,
                        &mut output,
                        null_mut(),
                    )
                };
                (code, output)
            })
            .join()
            .unwrap();
        assert_eq!(code, WRONG_THREAD);
        assert_eq!(untouched, 123);
    });
    assert_eq!(build(&mut c, settings, 0, 0.0, expiry, &mut out), 0);
    unsafe {
        assert_eq!(itofin_handle_release(&mut c, out, null_mut()), 0);
    }
    unsafe {
        assert_eq!(itofin_handle_release(&mut c, settings, null_mut()), 0);
    }
    assert_eq!(
        build(&mut c, settings, 0, 100.0, expiry, &mut out),
        INVALID_HANDLE
    );
}

#[test]
fn percentage_strike_uses_fd_and_retains_dependencies() {
    let mut c = Context::new();
    let settings = shared(Settings::<Date>::new());
    let today = Date::new(15, Month::June, 2026);
    settings.set_evaluation_date(today);
    let settings_id = c.insert(Shared::clone(&settings)).unwrap();
    let mut dc = 0;
    let mut process = 0;
    let mut engine = 0;
    unsafe {
        assert_eq!(itofin_day_counter_new(&mut c, 0, &mut dc, null_mut()), 0);
        assert_eq!(
            itofin_black_scholes_new(
                &mut c,
                100.0,
                0.05,
                0.02,
                0.2,
                today.serial_number(),
                dc,
                &mut process,
                null_mut()
            ),
            0
        );
        assert_eq!(
            itofin_fd_black_scholes_engine_new(
                &mut c,
                process,
                ItofinFdConfig::default(),
                &mut engine,
                null_mut()
            ),
            0
        );
    }
    for kind in [0, 1] {
        let mut option = 0;
        assert_eq!(
            build(
                &mut c,
                settings_id,
                kind,
                0.8,
                today.serial_number() + 360,
                &mut option
            ),
            0
        );
        {
            let retained = c.get::<SharedMut<VanillaOption>>(option).unwrap();
            let retained = retained.borrow();
            assert_eq!(retained.payoff().name(), "PercentageStrike");
            let factor = if kind == 0 { 0.2 } else { 0.0 };
            assert!((retained.payoff().strike() - 0.8).abs() < 1e-15);
            for price in [-100.0, 0.0, 104.0, 106.0] {
                assert!((retained.payoff().value(price) - price * factor).abs() < 1e-12);
            }
        }
        let mut value = -123.0;
        unsafe {
            assert_eq!(
                itofin_option_value(&mut c, option, 0, &mut value, null_mut()),
                CORE_ERROR
            );
            assert_eq!(value, -123.0);
            assert_eq!(
                itofin_option_set_engine(&mut c, option, process, 0, 0, null_mut()),
                0
            );
            assert_eq!(
                itofin_option_value(&mut c, option, 0, &mut value, null_mut()),
                CORE_ERROR
            );
            assert_eq!(value, -123.0);
            assert_eq!(
                itofin_option_set_engine(&mut c, option, engine, 2, 0, null_mut()),
                0
            );
            for field in 0..4 {
                assert_eq!(
                    itofin_option_value(&mut c, option, field, &mut value, null_mut()),
                    0
                );
                assert!(value.is_finite());
            }
            for field in 4..9 {
                value = -123.0;
                assert_eq!(
                    itofin_option_value(&mut c, option, field, &mut value, null_mut()),
                    CORE_ERROR
                );
                assert_eq!(value, -123.0);
            }
            settings.set_evaluation_date(today + 360);
            assert_eq!(
                itofin_option_value(&mut c, option, 0, &mut value, null_mut()),
                0
            );
            assert_eq!(value, 0.0);
            settings.set_evaluation_date(today);
            assert_eq!(
                itofin_option_value(&mut c, option, 0, &mut value, null_mut()),
                0
            );
            assert!(value.is_finite() && (0.0..100.0).contains(&value));
            assert_eq!(itofin_handle_release(&mut c, option, null_mut()), 0);
        }
    }
    let mut option = 0;
    assert_eq!(
        build(
            &mut c,
            settings_id,
            0,
            0.8,
            today.serial_number() + 360,
            &mut option
        ),
        0
    );
    unsafe {
        assert_eq!(
            itofin_option_set_engine(&mut c, option, engine, 2, 0, null_mut()),
            0
        );
        for handle in [settings_id, process, dc, engine] {
            assert_eq!(itofin_handle_release(&mut c, handle, null_mut()), 0);
        }
        settings.set_evaluation_date(today + 1);
        let mut value = -123.0;
        assert_eq!(
            itofin_option_value(&mut c, option, 0, &mut value, null_mut()),
            0
        );
        assert!(value.is_finite() && value > 0.0);
        assert_eq!(itofin_handle_release(&mut c, option, null_mut()), 0);
        assert_eq!(
            itofin_option_value(&mut c, option, 0, &mut value, null_mut()),
            INVALID_HANDLE
        );
    }
}
