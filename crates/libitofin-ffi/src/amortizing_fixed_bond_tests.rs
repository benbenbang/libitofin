use crate::bond_api::*;
use crate::boundary::*;
use crate::calendar_api::NativeCalendar;
use crate::curves_api::itofin_flat_forward_new;
use crate::time_api::{ItofinScheduleConfig, itofin_schedule_new};
use libitofin::cashflow::Leg;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::time::{
    calendars::WeekendsOnly,
    date::{Date, Month},
    daycounters::actual360::Actual360,
};
use serde_json::Value;
use std::ptr::{null, null_mut};

fn serial(day: i32, year: i32) -> i32 {
    Date::new(day, Month::January, year).serial_number()
}
fn fixture() -> (Context, ItofinAmortizingFixedBondConfig) {
    let mut c = Context::new();
    let settings = shared(Settings::new());
    settings.set_evaluation_date(Date::from_serial(serial(18, 2024)));
    let settings = c.insert(settings).unwrap();
    let dc = c.insert(Actual360::new()).unwrap();
    let cal = c
        .insert(NativeCalendar {
            inner: WeekendsOnly::new(),
            horizon: None,
            first_year: 1901,
        })
        .unwrap();
    let mut schedule = 0;
    unsafe {
        assert_eq!(
            itofin_schedule_new(
                &mut c,
                ItofinScheduleConfig {
                    start: serial(17, 2024),
                    end: serial(17, 2027),
                    frequency: 0,
                    calendar: cal,
                    convention: 2,
                    rule: 0,
                    termination_convention: 2,
                },
                &mut schedule,
                null_mut()
            ),
            0
        );
    }
    (
        c,
        ItofinAmortizingFixedBondConfig {
            settlement_days: 2,
            settings,
            issue_date: serial(17, 2024),
            payment_convention: 1,
            calendar: cal,
            schedule,
            day_counter: dc,
            first_period_day_counter: 0,
            ex_coupon_calendar: cal,
            ex_coupon_length: 0,
            ex_coupon_unit: 0,
            ex_coupon_convention: 2,
            has_ex_coupon: 1,
            ex_coupon_end_of_month: 0,
        },
    )
}
fn near(got: f64, want: f64, tol: f64) {
    assert!(
        got.is_finite() && want.is_finite() && (got - want).abs() <= tol,
        "got {got:0.17} want {want:0.17}"
    );
}
#[test]
fn amortizing_fixed_ffi_matches_shared_native_fixture() {
    let mut lines = include_str!("../../../sdk/go/testdata/amortizing_fixed_native.jsonl").lines();
    let identity: Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    assert_eq!(identity["compiled_quantlib_version"], "1.43");
    assert_eq!(identity["source_is_compiled_wheel"], false);
    let mut count = 0;
    for line in lines {
        let row: Value = serde_json::from_str(line).unwrap();
        count += 1;
        let (mut c, mut a) = fixture();
        a.settlement_days = row["settlement_days"].as_u64().unwrap().try_into().unwrap();
        a.ex_coupon_length = row["ex_days"].as_i64().unwrap().try_into().unwrap();
        a.has_ex_coupon = u8::from(a.ex_coupon_length != 0);
        let settings = c.get::<Shared<Settings<Date>>>(a.settings).unwrap();
        settings.set_evaluation_date(Date::from_serial(serial(
            row["when"][0].as_i64().unwrap().try_into().unwrap(),
            row["when"][2].as_i64().unwrap().try_into().unwrap(),
        )));
        let balances = row["notionals"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_f64().unwrap())
            .collect::<Vec<_>>();
        let mut id = 0;
        unsafe {
            assert_eq!(
                itofin_amortizing_fixed_rate_bond_new(
                    &mut c,
                    a,
                    balances.as_ptr(),
                    balances.len(),
                    [0.045, 0.052, 0.039].as_ptr(),
                    3,
                    &mut id,
                    null_mut()
                ),
                0
            );
            for (key, rate) in [("initial", 0.031), ("updated", 0.067)] {
                let expected = &row[key];
                let mut curve = 0;
                let mut engine = 0;
                let dc = c.insert(Actual360::new()).unwrap();
                let settings_id = c.insert(settings.clone()).unwrap();
                assert_eq!(
                    itofin_flat_forward_new(
                        &mut c,
                        serial(18, 2024),
                        rate,
                        dc,
                        &mut curve,
                        null_mut()
                    ),
                    0
                );
                assert_eq!(
                    itofin_discounting_bond_engine_new(
                        &mut c,
                        curve,
                        settings_id,
                        -1,
                        &mut engine,
                        null_mut()
                    ),
                    0
                );
                assert_eq!(itofin_bond_set_engine(&mut c, id, engine, null_mut()), 0);
                for field in 0..6 {
                    let mut got = f64::NAN;
                    assert_eq!(
                        itofin_bond_real(&mut c, id, field, 0, &mut got, null_mut()),
                        0,
                        "{} {key} field {field}",
                        row["name"]
                    );
                    let want = expected["values"][usize::try_from(field).unwrap()]
                        .as_f64()
                        .unwrap();
                    near(got, want, 2e-10_f64.max(want.abs() * 2e-12));
                }
                for (field, key) in [(1, "settlement"), (2, "maturity")] {
                    let mut got = 0;
                    assert_eq!(
                        itofin_bond_integer(&mut c, id, field, 0, &mut got, null_mut()),
                        0
                    );
                    assert_eq!(i64::from(got), expected[key].as_i64().unwrap());
                }
                for (field, key) in [(1, "flows"), (2, "redemptions")] {
                    let mut leg = 0;
                    assert_eq!(
                        itofin_bond_handle(&mut c, id, field, &mut leg, null_mut()),
                        0
                    );
                    let leg = c.get::<Leg>(leg).unwrap();
                    let rows = expected[key].as_array().unwrap();
                    assert_eq!(leg.len(), rows.len());
                    for (flow, row) in leg.iter().zip(rows) {
                        assert_eq!(
                            i64::from(flow.date().serial_number()),
                            row[0].as_i64().unwrap()
                        );
                        near(flow.amount().unwrap(), row[1].as_f64().unwrap(), 2e-11);
                    }
                }
            }
        }
    }
    assert_eq!(count, 108);
}
#[test]
fn amortizing_fixed_ffi_errors_preserve_outputs_and_context() {
    for balances in [
        vec![],
        vec![1000.],
        vec![1000., 750., 400., 1.],
        vec![1000., 1001., 400.],
        vec![1000., 0., 0.],
        vec![1000., f64::NAN, 400.],
        vec![f64::INFINITY, 750., 400.],
    ] {
        let (mut c, a) = fixture();
        let mut out = 987;
        unsafe {
            assert_ne!(
                itofin_amortizing_fixed_rate_bond_new(
                    &mut c,
                    a,
                    balances.as_ptr(),
                    balances.len(),
                    [0.045].as_ptr(),
                    1,
                    &mut out,
                    null_mut()
                ),
                0
            );
        }
        assert_eq!(out, 987);
    }
    for kind in 0..9 {
        let (mut c, mut a) = fixture();
        let mut out = 987;
        if kind == 0 {
            a.settings = a.day_counter;
        }
        if kind == 1 {
            a.schedule = a.day_counter;
        }
        if kind == 2 {
            a.day_counter = a.settings;
        }
        if kind == 3 {
            a.has_ex_coupon = 2;
        }
        if kind == 4 {
            a.ex_coupon_end_of_month = 2;
        }
        if kind == 5 {
            a.ex_coupon_length = -1;
        }
        if kind == 6 {
            a.issue_date = i32::MAX;
        }
        if kind == 7 {
            a.payment_convention = 999;
        }
        if kind == 8 {
            a.settlement_days = u32::MAX;
        }
        unsafe {
            assert_ne!(
                itofin_amortizing_fixed_rate_bond_new(
                    &mut c,
                    a,
                    [1000., 750., 400.].as_ptr(),
                    3,
                    [0.045].as_ptr(),
                    1,
                    &mut out,
                    null_mut()
                ),
                0
            );
        }
        assert_eq!(out, 987);
    }
    let (mut c, a) = fixture();
    let mut out = 987;
    unsafe {
        assert_eq!(
            itofin_amortizing_fixed_rate_bond_new(
                &mut c,
                a,
                null(),
                3,
                [0.045].as_ptr(),
                1,
                &mut out,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
    }
    assert_eq!(out, 987);
    let (mut c, a) = fixture();
    unsafe {
        assert_eq!(
            itofin_amortizing_fixed_rate_bond_new(
                &mut c,
                a,
                [1000., 750., 400.].as_ptr(),
                3,
                [0.045].as_ptr(),
                1,
                null_mut(),
                null_mut()
            ),
            INVALID_ARGUMENT
        );
    }
    let (mut c, a) = fixture();
    unsafe {
        assert_eq!(
            itofin_amortizing_fixed_rate_bond_new(
                &mut c,
                a,
                [1000., 750., 400.].as_ptr(),
                3,
                [0.045].as_ptr(),
                1,
                &mut out,
                null_mut()
            ),
            0
        );
    }
    assert_ne!(out, 987);
}

#[test]
fn amortizing_fixed_ffi_foreign_thread_and_nonfinite_coupon_do_not_poison() {
    let (mut c, a) = fixture();
    let address = (&mut c as *mut Context) as usize;
    let (status, out) = std::thread::spawn(move || {
        let mut out = 987;
        let status = unsafe {
            itofin_amortizing_fixed_rate_bond_new(
                address as *mut Context,
                a,
                [1000., 750., 400.].as_ptr(),
                3,
                [0.045].as_ptr(),
                1,
                &mut out,
                null_mut(),
            )
        };
        (status, out)
    })
    .join()
    .unwrap();
    assert_eq!(status, WRONG_THREAD);
    assert_eq!(out, 987);
    for rate in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, f64::MAX] {
        let (mut c, a) = fixture();
        let mut out = 987;
        unsafe {
            assert_ne!(
                itofin_amortizing_fixed_rate_bond_new(
                    &mut c,
                    a,
                    [1000., 750., 400.].as_ptr(),
                    3,
                    [rate].as_ptr(),
                    1,
                    &mut out,
                    null_mut()
                ),
                0
            );
        }
        assert_eq!(out, 987);
    }
    let (mut c, a) = fixture();
    let mut out = 0;
    unsafe {
        assert_eq!(
            itofin_amortizing_fixed_rate_bond_new(
                &mut c,
                a,
                [1000., 750., 400.].as_ptr(),
                3,
                [0.045].as_ptr(),
                1,
                &mut out,
                null_mut()
            ),
            0
        );
    }
}

#[test]
fn amortizing_fixed_misaligned_error_preserves_output_and_native_owners() {
    let (mut c, a) = fixture();
    let settings = c.get::<Shared<Settings<Date>>>(a.settings).unwrap();
    let owners = Shared::strong_count(&settings);
    let mut storage = [0x5A5A_5A5A_u32; size_of::<ItofinError>() / size_of::<u32>() + 1];
    let before = storage;
    let error = storage.as_mut_ptr().cast::<u8>();
    let error = error.wrapping_add(1).cast::<ItofinError>();
    assert!(!error.is_aligned());
    let mut out = 777;
    let status = unsafe {
        itofin_amortizing_fixed_rate_bond_new(
            &mut c,
            a,
            [1000., 750., 400.].as_ptr(),
            3,
            [0.045].as_ptr(),
            1,
            &mut out,
            error,
        )
    };
    assert_eq!(status, INVALID_ARGUMENT);
    assert_eq!(out, 777);
    assert_eq!(storage, before);
    assert_eq!(Shared::strong_count(&settings), owners);
}
