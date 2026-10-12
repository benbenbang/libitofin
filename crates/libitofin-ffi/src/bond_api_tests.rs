use crate::bond_api::*;
use crate::boundary::*;
use crate::calendar_api::NativeCalendar;
use crate::rates_api::settings;
use crate::settings_api::{itofin_settings_new, itofin_settings_set_include_reference_date_events};
use crate::time_api::{
    ItofinScheduleConfig, itofin_calendar_new, itofin_day_counter_new, itofin_schedule_new,
};
use libitofin::shared::Shared;
use libitofin::time::schedule::Schedule;
use std::ptr::{null, null_mut};

fn context() -> (Context, u64, u64, u64, u64) {
    let mut c = Context::new();
    let (mut settings_id, mut calendar, mut counter, mut schedule) = (0, 0, 0, 0);
    unsafe {
        assert_eq!(itofin_settings_new(&mut c, &mut settings_id, null_mut()), 0);
        settings(&c, settings_id)
            .unwrap()
            .set_evaluation_date(crate::time_api::date(45_306).unwrap());
        assert_eq!(itofin_calendar_new(&mut c, 0, &mut calendar, null_mut()), 0);
        assert_eq!(
            itofin_day_counter_new(&mut c, 0, &mut counter, null_mut()),
            0
        );
        assert_eq!(
            itofin_schedule_new(
                &mut c,
                ItofinScheduleConfig {
                    start: 45_306,
                    end: 46_402,
                    frequency: 1,
                    calendar,
                    convention: 2,
                    rule: 0,
                    termination_convention: 2
                },
                &mut schedule,
                null_mut()
            ),
            0
        );
    }
    (c, settings_id, calendar, counter, schedule)
}
fn common(settings: u64, calendar: u64) -> ItofinBondConfig {
    ItofinBondConfig {
        settlement_days: 2,
        face_amount: 1000.0,
        redemption: 100.0,
        settings,
        issue_date: 45_306,
        payment_convention: 1,
        calendar,
    }
}
fn fixed(settings: u64, counter: u64, schedule: u64) -> ItofinFixedBondConfig {
    ItofinFixedBondConfig {
        bond: common(settings, 0),
        schedule,
        day_counter: counter,
        first_period_day_counter: 0,
        ex_coupon_calendar: 0,
        ex_coupon_length: 0,
        ex_coupon_unit: 0,
        ex_coupon_convention: 2,
        has_ex_coupon: 0,
        ex_coupon_end_of_month: 0,
    }
}
#[test]
fn bond_native_handles_metadata_and_errors_are_recoverable() {
    let (mut c, settings_id, calendar, counter, schedule) = context();
    let retained_calendar = c.metadata::<NativeCalendar>(schedule).unwrap();
    assert_eq!(
        retained_calendar.inner.name(),
        c.get::<NativeCalendar>(calendar).unwrap().inner.name()
    );
    assert!(c.get::<Schedule>(schedule).is_ok());
    assert!(c.metadata::<String>(schedule).is_err());
    let mut bond = 0;
    unsafe {
        assert_eq!(itofin_handle_release(&mut c, calendar, null_mut()), 0);
        assert_eq!(
            itofin_fixed_rate_bond_new(
                &mut c,
                fixed(settings_id, counter, schedule),
                [0.05].as_ptr(),
                1,
                &mut bond,
                null_mut()
            ),
            0
        );
        assert_eq!(itofin_handle_release(&mut c, schedule, null_mut()), 0);
        assert!(c.metadata::<NativeCalendar>(schedule).is_err());
        assert_eq!(
            itofin_handle_release(&mut c, schedule, null_mut()),
            INVALID_HANDLE
        );
        let mut value = 0.0;
        assert_eq!(
            itofin_bond_real(&mut c, bond, 0, 0, &mut value, null_mut()),
            CORE_ERROR
        );
        assert_eq!(
            itofin_bond_real(&mut c, bond, 4, 0, &mut value, null_mut()),
            0
        );
        assert_eq!(
            itofin_bond_real(&mut c, bond, 4, i32::MAX, &mut value, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_bond_real(&mut c, counter, 4, 0, &mut value, null_mut()),
            INVALID_HANDLE
        );
        assert_eq!(
            itofin_bond_real(&mut c, bond, 4, 0, null_mut(), null_mut()),
            INVALID_ARGUMENT
        );
        let mut output = 0;
        assert_eq!(
            itofin_bond_integer(&mut c, bond, 6, 0, &mut output, null_mut()),
            0
        );
        assert_eq!(output, 1);
        assert_eq!(
            itofin_settings_set_include_reference_date_events(&mut c, settings_id, 2, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_bond_integer(&mut c, bond, 1, 0, &mut output, null_mut()),
            0
        );
        assert_eq!(output, 45_308);
        settings(&c, settings_id)
            .unwrap()
            .set_evaluation_date(crate::time_api::date(109_574).unwrap());
        assert_eq!(
            itofin_bond_integer(&mut c, bond, 1, 0, &mut output, null_mut()),
            INVALID_ARGUMENT
        );
        settings(&c, settings_id)
            .unwrap()
            .set_evaluation_date(crate::time_api::date(45_306).unwrap());
        assert_eq!(
            itofin_bond_integer(&mut c, bond, 1, 0, &mut output, null_mut()),
            0
        );
        let mut leg = 0;
        assert_eq!(itofin_bond_handle(&mut c, bond, 2, &mut leg, null_mut()), 0);
        assert_eq!(itofin_handle_release(&mut c, bond, null_mut()), 0);
        assert!(c.get::<libitofin::cashflow::Leg>(leg).is_ok());
    }
}
#[test]
fn bond_native_invalid_constructors_and_foreign_thread_leave_context_healthy() {
    let (mut c, settings_id, calendar, counter, schedule) = context();
    let mut out = 0;
    unsafe {
        assert_eq!(
            itofin_fixed_rate_bond_new(
                &mut c,
                fixed(settings_id, counter, schedule),
                null(),
                1,
                &mut out,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_fixed_rate_bond_new(
                &mut c,
                fixed(settings_id, counter, schedule),
                null(),
                0,
                &mut out,
                null_mut()
            ),
            CORE_ERROR
        );
        assert_eq!(
            itofin_fixed_rate_bond_new(
                &mut c,
                fixed(settings_id, counter, schedule),
                [f64::NAN].as_ptr(),
                1,
                &mut out,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        let mut config = fixed(settings_id, counter, schedule);
        config.has_ex_coupon = 2;
        assert_eq!(
            itofin_fixed_rate_bond_new(&mut c, config, [0.05].as_ptr(), 1, &mut out, null_mut()),
            INVALID_ARGUMENT
        );
        for face in [0.0, -1.0, f64::INFINITY, f64::NAN] {
            let mut config = common(settings_id, calendar);
            config.face_amount = face;
            assert_eq!(
                itofin_zero_coupon_bond_new(&mut c, config, 46_402, &mut out, null_mut()),
                INVALID_ARGUMENT
            );
        }
        assert_eq!(
            itofin_zero_coupon_bond_new(
                &mut c,
                common(settings_id, calendar),
                i32::MAX,
                &mut out,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        let mut config = common(settings_id, calendar);
        config.settlement_days = u32::MAX;
        assert_eq!(
            itofin_zero_coupon_bond_new(&mut c, config, 46_402, &mut out, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_zero_coupon_bond_new(
                &mut c,
                common(settings_id, calendar),
                46_402,
                &mut out,
                null_mut()
            ),
            0
        );
    }
    let address = (&mut c as *mut Context) as usize;
    let id = out;
    let status = std::thread::spawn(move || unsafe {
        itofin_handle_release(address as *mut Context, id, null_mut())
    })
    .join()
    .unwrap();
    assert_eq!(status, WRONG_THREAD);
    let native = c.get::<NativeBond>(out).unwrap();
    assert_eq!(native.settlement(None).unwrap().serial_number(), 45_308);
    assert_eq!(
        Shared::strong_count(&native.bond.borrow().settings_handle()),
        3
    );
}

#[test]
fn context_metadata_released_with_handle_and_context() {
    let owner = Shared::new(std::cell::Cell::new(0));
    let mut c = Context::new();
    let id = c.insert_with_metadata(42_u64, owner.clone()).unwrap();
    assert_eq!(c.get::<u64>(id).unwrap(), 42);
    assert_eq!(Shared::strong_count(&owner), 2);
    assert!(
        Context::new()
            .metadata::<Shared<std::cell::Cell<i32>>>(id)
            .is_err()
    );
    let recovered = c.metadata::<Shared<std::cell::Cell<i32>>>(id).unwrap();
    assert!(Shared::ptr_eq(&owner, &recovered));
    drop(recovered);
    unsafe {
        assert_eq!(itofin_handle_release(&mut c, id, null_mut()), 0);
    }
    assert_eq!(Shared::strong_count(&owner), 1);
    let _ = c.insert_with_metadata(43_u64, owner.clone()).unwrap();
    assert_eq!(Shared::strong_count(&owner), 2);
    drop(c);
    assert_eq!(Shared::strong_count(&owner), 1);
}

#[test]
fn first_accrual_date_is_not_adjusted_as_a_payment() {
    let (mut c, settings_id, payment_calendar, counter, _) = context();
    let calendar = NativeCalendar {
        inner: libitofin::time::calendars::NullCalendar::new(),
        horizon: None,
        first_year: 1901,
    };
    let schedule = Schedule::from_dates(vec![
        crate::time_api::date(367).unwrap(),
        crate::time_api::date(732).unwrap(),
    ]);
    let schedule_id = c.insert_with_metadata(schedule, calendar).unwrap();
    let mut config = fixed(settings_id, counter, schedule_id);
    config.bond.calendar = payment_calendar;
    config.bond.issue_date = 0;
    config.bond.payment_convention = 3;
    let mut out = 0;
    unsafe {
        assert_eq!(
            itofin_fixed_rate_bond_new(&mut c, config, [0.05].as_ptr(), 1, &mut out, null_mut()),
            0
        );
    }
    let bond = c.get::<NativeBond>(out).unwrap();
    assert_eq!(
        bond.bond.borrow().cashflows()[0].date().serial_number(),
        731
    );
}
