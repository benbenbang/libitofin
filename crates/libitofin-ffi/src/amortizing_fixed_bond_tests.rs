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
use std::ptr::null_mut;

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
