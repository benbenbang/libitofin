use super::*;
use crate::bond_api::{itofin_bond_handle, itofin_bond_integer};
use crate::cashflows_api::{itofin_cashflow_amount, itofin_leg_item};
use libitofin::currency::Currency;
use libitofin::indexes::IborIndex;
use libitofin::time::{calendars::NullCalendar, daycounters::actual360::Actual360};
use std::ptr::{null, null_mut};

struct Fixture {
    c: Context,
    settings: u64,
    index: u64,
    schedule: u64,
    counter: u64,
}
impl Fixture {
    fn new(periods: usize, start: i32) -> Self {
        let mut c = Context::new();
        let cal = crate::calendar_api::NativeCalendar {
            inner: NullCalendar::new(),
            horizon: None,
            first_year: 1901,
        };
        let settings_value = shared(Settings::new());
        settings_value.set_evaluation_date(crate::time_api::date(45_306).unwrap());
        let index_value = shared(IborIndex::new(
            "FloatingNative".into(),
            Period::new(6, TimeUnit::Months),
            2,
            Currency::usd(),
            cal.inner.clone(),
            BusinessDayConvention::ModifiedFollowing,
            false,
            Actual360::new(),
            Handle::empty(),
            settings_value.clone(),
        ));
        let index = c.insert(NativeIbor::builtin(index_value)).unwrap();
        let settings = c.insert(settings_value).unwrap();
        let counter = c.insert(Actual360::new()).unwrap();
        let schedule = Schedule::from_dates(
            (0..=periods)
                .map(|i| crate::time_api::date(start + 30 * i as i32).unwrap())
                .collect(),
        );
        let schedule = c.insert_with_metadata(schedule, cal).unwrap();
        Self {
            c,
            settings,
            index,
            schedule,
            counter,
        }
    }
    fn config(&self) -> ItofinFloatingBondConfig {
        ItofinFloatingBondConfig {
            bond: ItofinBondConfig {
                settlement_days: 2,
                face_amount: 1000.,
                redemption: 100.,
                settings: self.settings,
                issue_date: 0,
                payment_convention: 1,
                calendar: 0,
            },
            schedule: self.schedule,
            index: self.index,
            day_counter: self.counter,
            fixing_days: 0,
            has_fixing_days: 0,
            fixing_convention: 3,
            ex_coupon_calendar: 0,
            ex_coupon_length: 0,
            ex_coupon_unit: 0,
            ex_coupon_convention: 2,
            has_ex_coupon: 0,
            ex_coupon_end_of_month: 0,
        }
    }
}
fn vectors() -> ItofinFloatingBondVectors {
    ItofinFloatingBondVectors {
        gearings: null(),
        gearings_count: 0,
        spreads: null(),
        spreads_count: 0,
        caps: null(),
        caps_count: 0,
        floors: null(),
        floors_count: 0,
    }
}

fn amortizing_config(f: &Fixture) -> ItofinAmortizingFloatingBondConfig {
    ItofinAmortizingFloatingBondConfig {
        settlement_days: 0,
        schedule: f.schedule,
        index: f.index,
        day_counter: f.counter,
        settings: f.settings,
        payment_convention: 1,
        issue_date: 0,
        fixing_days: 0,
        has_fixing_days: 0,
        fixing_convention: 3,
        ex_coupon_calendar: 0,
        ex_coupon_length: 0,
        ex_coupon_unit: 0,
        ex_coupon_convention: 2,
        has_ex_coupon: 0,
        ex_coupon_end_of_month: 0,
    }
}

#[test]
fn amortizing_cashflow_positions_retain_coupon_guards_but_principal_has_none() {
    let start = Date::new(5, libitofin::time::date::Month::October, 2026).serial_number();
    let mut f = Fixture::new(4, start);
    let mut index = f.c.get::<NativeIbor>(f.index).unwrap();
    index.maturity_calendar.horizon = Some(2026);
    f.index = f.c.insert(index).unwrap();
    let balances = [1000., 800., 800., 500.];
    let mut id = 0;
    let config = amortizing_config(&f);
    unsafe {
        assert_eq!(
            itofin_amortizing_floating_rate_bond_new(
                &mut f.c,
                config,
                balances.as_ptr(),
                balances.len(),
                null(),
                0,
                &mut id,
                null_mut()
            ),
            0
        );
    }
    let native = f.c.get::<NativeBond>(id).unwrap();
    let mapping = &native.floating.as_ref().unwrap().coupons;
    assert_eq!(
        mapping.iter().map(Option::is_some).collect::<Vec<_>>(),
        vec![true, false, true, true, false, true, false]
    );
    let mut leg = 0;
    unsafe {
        assert_eq!(itofin_bond_handle(&mut f.c, id, 1, &mut leg, null_mut()), 0);
        for handle in [id, f.index, f.schedule, f.counter] {
            assert_eq!(itofin_handle_release(&mut f.c, handle, null_mut()), 0);
        }
        for (position, expected) in [(1, 200.), (4, 300.), (6, 500.)] {
            let mut flow = 0;
            let mut amount: f64 = -91.;
            assert_eq!(
                itofin_leg_item(&mut f.c, leg, position, &mut flow, null_mut()),
                0
            );
            assert_eq!(
                itofin_cashflow_amount(&mut f.c, flow, &mut amount, null_mut()),
                0
            );
            assert!(amount.is_finite() && (amount - expected).abs() <= 1e-12);
        }
        for position in [0, 2, 3, 5] {
            let mut flow = 0;
            let mut amount = -91.;
            assert_eq!(
                itofin_leg_item(&mut f.c, leg, position, &mut flow, null_mut()),
                0
            );
            assert_eq!(
                itofin_cashflow_amount(&mut f.c, flow, &mut amount, null_mut()),
                INVALID_ARGUMENT
            );
            assert_eq!(amount, -91.);
        }
    }
    let retained = native.floating.unwrap().index;
    let settings = f.c.get::<Shared<Settings<Date>>>(f.settings).unwrap();
    settings.set_evaluation_date(Date::new(1, libitofin::time::date::Month::January, 2028));
    for (i, balance) in balances.into_iter().enumerate() {
        retained
            .inner
            .add_fixing(
                crate::time_api::date(start + i as i32 * 30 - 2).unwrap(),
                0.037,
            )
            .unwrap();
        let position = [0, 2, 3, 5][i];
        let mut flow = 0;
        let mut amount: f64 = -91.;
        unsafe {
            assert_eq!(
                itofin_leg_item(&mut f.c, leg, position, &mut flow, null_mut()),
                0
            );
            assert_eq!(
                itofin_cashflow_amount(&mut f.c, flow, &mut amount, null_mut()),
                0
            );
        }
        assert!(amount.is_finite() && (amount - balance * 0.037 * 30. / 360.).abs() < 3e-12);
    }
}

#[test]
fn amortizing_constructor_rejects_invalid_inputs_without_writing_output() {
    let mut f = Fixture::new(4, 46_500);
    for balances in [
        vec![],
        vec![1000.],
        vec![1000.; 5],
        vec![1000., 0., 800., 500.],
        vec![1000., -1., 800., 500.],
        vec![1000., 1100., 800., 500.],
        vec![1000., f64::NAN, 800., 500.],
        vec![f64::INFINITY, 800., 800., 500.],
    ] {
        let mut output = 777;
        let config = amortizing_config(&f);
        unsafe {
            assert_ne!(
                itofin_amortizing_floating_rate_bond_new(
                    &mut f.c,
                    config,
                    balances.as_ptr(),
                    balances.len(),
                    null(),
                    0,
                    &mut output,
                    null_mut()
                ),
                0
            );
        }
        assert_eq!(output, 777);
    }
    let balances = [1000.; 4];
    let mut output = 777;
    let config = amortizing_config(&f);
    unsafe {
        assert_eq!(
            itofin_amortizing_floating_rate_bond_new(
                &mut f.c,
                config,
                null(),
                4,
                null(),
                0,
                &mut output,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
    }
    assert_eq!(output, 777);
    let config = amortizing_config(&f);
    unsafe {
        assert_eq!(
            itofin_amortizing_floating_rate_bond_new(
                &mut f.c,
                config,
                balances.as_ptr(),
                4,
                null(),
                0,
                null_mut(),
                null_mut()
            ),
            INVALID_ARGUMENT
        );
    }
    let mut config = amortizing_config(&f);
    config.index = f.settings;
    unsafe {
        assert_eq!(
            itofin_amortizing_floating_rate_bond_new(
                &mut f.c,
                config,
                balances.as_ptr(),
                4,
                null(),
                0,
                &mut output,
                null_mut()
            ),
            INVALID_HANDLE
        );
    }
    assert_eq!(output, 777);
}

#[test]
fn amortizing_constructor_confines_the_context_to_its_owner_thread() {
    let mut f = Fixture::new(4, 46_500);
    let config = amortizing_config(&f);
    let address = (&mut f.c as *mut Context) as usize;
    let (status, output) = std::thread::spawn(move || {
        let balances = [1000.; 4];
        let mut output = 777;
        let status = unsafe {
            itofin_amortizing_floating_rate_bond_new(
                address as *mut Context,
                config,
                balances.as_ptr(),
                4,
                null(),
                0,
                &mut output,
                null_mut(),
            )
        };
        (status, output)
    })
    .join()
    .unwrap();
    assert_eq!(status, WRONG_THREAD);
    assert_eq!(output, 777);
}

#[test]
fn amortizing_bad_error_alignment_does_not_allocate_or_write_output() {
    let mut f = Fixture::new(4, 46_500);
    let index = f.c.get::<NativeIbor>(f.index).unwrap().inner;
    let settings = f.c.get::<Shared<Settings<Date>>>(f.settings).unwrap();
    let owners = (
        Shared::strong_count(&index),
        Shared::strong_count(&settings),
    );
    let mut bytes =
        vec![0u8; std::mem::size_of::<ItofinError>() + std::mem::align_of::<ItofinError>()];
    let alignment = std::mem::align_of::<ItofinError>();
    let offset = (alignment - bytes.as_ptr() as usize % alignment) % alignment + 1;
    let error = bytes.as_mut_ptr().wrapping_add(offset) as *mut ItofinError;
    let config = amortizing_config(&f);
    let balances = [1000.; 4];
    let mut output = 777;
    unsafe {
        assert_eq!(
            itofin_amortizing_floating_rate_bond_new(
                &mut f.c,
                config,
                balances.as_ptr(),
                4,
                null(),
                0,
                &mut output,
                error
            ),
            INVALID_ARGUMENT
        );
    }
    assert_eq!(output, 777);
    assert!(bytes.iter().all(|&byte| byte == 0));
    assert_eq!(
        owners,
        (
            Shared::strong_count(&index),
            Shared::strong_count(&settings)
        )
    );
}

#[test]
fn amortizing_date_span_bounds_are_typed_errors_with_unchanged_outputs() {
    let mut f = Fixture::new(4, 46_500);
    let bound = (Date::max_date() - Date::min_date()) as u32;
    let balances = [1000.; 4];
    for (settlement, fixing) in [(bound + 1, 0), (u32::MAX, 0), (0, bound + 1), (0, u32::MAX)] {
        let mut config = amortizing_config(&f);
        config.settlement_days = settlement;
        config.has_fixing_days = 1;
        config.fixing_days = fixing;
        let mut output = 777;
        unsafe {
            assert_eq!(
                itofin_amortizing_floating_rate_bond_new(
                    &mut f.c,
                    config,
                    balances.as_ptr(),
                    4,
                    null(),
                    0,
                    &mut output,
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
        }
        assert_eq!(output, 777);
    }
    let settings = f.c.get::<Shared<Settings<Date>>>(f.settings).unwrap();
    let index = shared(IborIndex::new(
        "OversizedAmortizingNative".into(),
        Period::new(6, TimeUnit::Months),
        bound + 1,
        Currency::usd(),
        NullCalendar::new(),
        BusinessDayConvention::Following,
        false,
        Actual360::new(),
        Handle::empty(),
        settings,
    ));
    let mut config = amortizing_config(&f);
    config.index = f.c.insert(NativeIbor::builtin(index)).unwrap();
    config.has_fixing_days = 1;
    config.fixing_days = 0;
    let mut output = 777;
    unsafe {
        assert_eq!(
            itofin_amortizing_floating_rate_bond_new(
                &mut f.c,
                config,
                balances.as_ptr(),
                4,
                null(),
                0,
                &mut output,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
    }
    assert_eq!(output, 777);
}

#[test]
fn amortizing_same_date_coupons_and_principals_keep_allocation_identity() {
    use libitofin::time::{calendars::WeekendsOnly, date::Month};
    let mut f = Fixture::new(3, 46_500);
    let calendar = crate::calendar_api::NativeCalendar {
        inner: WeekendsOnly::new(),
        horizon: None,
        first_year: 1901,
    };
    let schedule = Schedule::with_metadata(
        (2..=5)
            .map(|day| Date::new(day, Month::October, 2026))
            .collect(),
        calendar.inner.clone(),
        BusinessDayConvention::Unadjusted,
        None,
        None,
        None,
        None,
        vec![],
    );
    f.schedule = f.c.insert_with_metadata(schedule, calendar).unwrap();
    let config = amortizing_config(&f);
    let balances = [1000., 800., 500.];
    let mut id = 0;
    unsafe {
        assert_eq!(
            itofin_amortizing_floating_rate_bond_new(
                &mut f.c,
                config,
                balances.as_ptr(),
                3,
                null(),
                0,
                &mut id,
                null_mut()
            ),
            0
        );
    }
    let native = f.c.get::<NativeBond>(id).unwrap();
    let bond = native.bond.borrow();
    assert!(
        bond.cashflows()
            .iter()
            .all(|f| f.date() == Date::new(5, Month::October, 2026))
    );
    let mapping = &native.floating.as_ref().unwrap().coupons;
    assert_eq!(
        mapping.iter().map(Option::is_some).collect::<Vec<_>>(),
        vec![true, true, true, false, false, false]
    );
    for (i, expected) in [1000., 800., 500.].into_iter().enumerate() {
        assert_eq!(mapping[i].as_ref().unwrap().nominal(), expected);
        assert_eq!(
            mapping[i].as_ref().unwrap().accrual_start_date(),
            Date::new(i as i32 + 2, Month::October, 2026)
        );
    }
    for (i, expected) in [200., 300., 500.].into_iter().enumerate() {
        assert_eq!(bond.cashflows()[i + 3].amount().unwrap(), expected);
    }
}

#[test]
fn floating_constructor_errors_preserve_outputs_and_healthy_context() {
    let mut f = Fixture::new(2, 45_500);
    let mut out = 999;
    unsafe {
        let mut config = f.config();
        config.has_fixing_days = 2;
        assert_eq!(
            itofin_floating_rate_bond_new(&mut f.c, config, vectors(), &mut out, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(out, 999);
        let mut bad = vectors();
        bad.gearings_count = 3;
        let config = f.config();
        assert_eq!(
            itofin_floating_rate_bond_new(&mut f.c, config, bad, &mut out, null_mut()),
            INVALID_ARGUMENT
        );
        let mut bad = vectors();
        bad.spreads_count = 1;
        let config = f.config();
        assert_eq!(
            itofin_floating_rate_bond_new(&mut f.c, config, bad, &mut out, null_mut()),
            INVALID_ARGUMENT
        );
        let mut bad = vectors();
        bad.spreads_count = 1;
        bad.spreads = [f64::NAN].as_ptr();
        let config = f.config();
        assert_eq!(
            itofin_floating_rate_bond_new(&mut f.c, config, bad, &mut out, null_mut()),
            INVALID_ARGUMENT
        );
        let config = f.config();
        assert_eq!(
            itofin_floating_rate_bond_new(&mut f.c, config, vectors(), null_mut(), null_mut()),
            INVALID_ARGUMENT
        );
        let config = f.config();
        assert_eq!(
            itofin_floating_rate_bond_new(&mut f.c, config, vectors(), &mut out, null_mut()),
            0
        );
        assert_ne!(out, 999);
        let mut wrong = 0;
        assert_eq!(
            itofin_floating_bond_index(&mut f.c, f.counter, &mut wrong, null_mut()),
            INVALID_HANDLE
        );
        assert_eq!(
            itofin_floating_bond_set_pricer(&mut f.c, out, f.counter, null_mut()),
            INVALID_HANDLE
        );
        let mut value = 0;
        assert_eq!(
            itofin_bond_integer(&mut f.c, out, 1, 0, &mut value, null_mut()),
            0
        );
        let mut pricer = 0;
        assert_eq!(
            itofin_black_ibor_coupon_pricer_new(&mut f.c, 0, &mut pricer, null_mut()),
            0
        );
        assert_eq!(
            itofin_floating_bond_set_pricer(&mut f.c, out, pricer, null_mut()),
            0
        );
        assert_eq!(itofin_handle_release(&mut f.c, pricer, null_mut()), 0);
        assert_eq!(
            itofin_floating_bond_set_pricer(&mut f.c, out, pricer, null_mut()),
            INVALID_HANDLE
        );
    }
}

#[test]
fn floating_large_leg_clones_share_immutable_storage_and_live_pricer_cells() {
    let mut f = Fixture::new(512, 45_500);
    let config = f.config();
    let mut id = 0;
    unsafe {
        assert_eq!(
            itofin_floating_rate_bond_new(&mut f.c, config, vectors(), &mut id, null_mut()),
            0
        );
    }
    let state = f.c.get::<NativeBond>(id).unwrap().floating.unwrap();
    let duplicate = f.c.get::<NativeBond>(id).unwrap().floating.unwrap();
    assert!(Shared::ptr_eq(&state.coupons, &duplicate.coupons));
    assert!(Shared::ptr_eq(&state.optionlets, &duplicate.optionlets));
    assert!(Shared::ptr_eq(&state.pricer_guard, &duplicate.pricer_guard));
    let mut leg = 0;
    unsafe {
        assert_eq!(itofin_bond_handle(&mut f.c, id, 1, &mut leg, null_mut()), 0);
    }
    let mut handles = Vec::new();
    for i in 0..512 {
        let mut flow = 0;
        unsafe {
            assert_eq!(itofin_leg_item(&mut f.c, leg, i, &mut flow, null_mut()), 0);
        }
        let guard = f.c.metadata::<NativeFloatingCashflow>(flow).unwrap();
        assert!(Shared::ptr_eq(&state.coupons, &guard.floating.coupons));
        assert!(Shared::ptr_eq(
            &state.optionlets,
            &guard.floating.optionlets
        ));
        assert!(Shared::ptr_eq(
            &state.pricer_guard,
            &guard.floating.pricer_guard
        ));
        assert_eq!(guard.coupon, i as usize);
        handles.push(flow);
    }
    assert_eq!(Shared::strong_count(&state.coupons), 516);
    unsafe {
        assert_eq!(itofin_handle_release(&mut f.c, id, null_mut()), 0);
        assert_eq!(itofin_handle_release(&mut f.c, leg, null_mut()), 0);
        for id in handles {
            assert_eq!(itofin_handle_release(&mut f.c, id, null_mut()), 0);
        }
    }
    assert_eq!(Shared::strong_count(&state.coupons), 2);
}

#[test]
fn floating_zero_gearing_at_date_min_and_historical_maturity_at_date_max_are_safe() {
    let mut f = Fixture::new(1, 367);
    let mut config = f.config();
    config.fixing_days = 2;
    config.has_fixing_days = 1;
    let mut v = vectors();
    v.gearings = [0.0].as_ptr();
    v.gearings_count = 1;
    v.spreads = [0.01].as_ptr();
    v.spreads_count = 1;
    let mut id = 0;
    unsafe {
        assert_eq!(
            itofin_floating_rate_bond_new(&mut f.c, config, v, &mut id, null_mut()),
            0
        );
    }
    assert!(f.c.get::<NativeBond>(id).unwrap().floating.unwrap().coupons[0].is_none());
    let mut zero_leg = 0;
    let mut zero_flow = 0;
    let mut zero_amount = 0.0;
    unsafe {
        assert_eq!(
            itofin_bond_handle(&mut f.c, id, 1, &mut zero_leg, null_mut()),
            0
        );
        assert_eq!(
            itofin_leg_item(&mut f.c, zero_leg, 0, &mut zero_flow, null_mut()),
            0
        );
        assert_eq!(
            itofin_cashflow_amount(&mut f.c, zero_flow, &mut zero_amount, null_mut()),
            0
        );
    }
    assert!((zero_amount - 1000.0 * 0.01 * 30.0 / 360.0).abs() < 2e-12);
    let mut f = Fixture::new(1, 109_520);
    let config = f.config();
    let mut v = vectors();
    v.caps = [0.04].as_ptr();
    v.caps_count = 1;
    unsafe {
        assert_eq!(
            itofin_floating_rate_bond_new(&mut f.c, config, v, &mut id, null_mut()),
            0
        );
    }
    let mut leg = 0;
    let mut flow = 0;
    let mut pricer = 0;
    let mut amount = 123.0;
    unsafe {
        assert_eq!(
            itofin_black_ibor_coupon_pricer_new(&mut f.c, 0, &mut pricer, null_mut()),
            0
        );
        assert_eq!(
            itofin_floating_bond_set_pricer(&mut f.c, id, pricer, null_mut()),
            0
        );
        assert_eq!(itofin_bond_handle(&mut f.c, id, 1, &mut leg, null_mut()), 0);
        assert_eq!(itofin_leg_item(&mut f.c, leg, 0, &mut flow, null_mut()), 0);
        assert_eq!(
            itofin_cashflow_amount(&mut f.c, flow, &mut amount, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(amount, 123.0);
    }
    let index = f.c.get::<NativeIbor>(f.index).unwrap().inner;
    settings(&f.c, f.settings)
        .unwrap()
        .set_evaluation_date(crate::time_api::date(109_530).unwrap());
    index
        .add_fixing(crate::time_api::date(109_518).unwrap(), 0.03)
        .unwrap();
    unsafe {
        assert_eq!(
            itofin_cashflow_amount(&mut f.c, flow, &mut amount, null_mut()),
            0
        );
    }
    assert!((amount - 2.5).abs() < 1e-14);
}

#[test]
fn floating_calendar_guards_only_actual_holiday_queries() {
    let calendar = crate::calendar_api::NativeCalendar {
        inner: NullCalendar::new(),
        horizon: Some(2014),
        first_year: 2012,
    };
    let end = crate::time_api::date(42_004).unwrap();
    let future = advance(
        &calendar,
        end,
        3,
        TimeUnit::Months,
        BusinessDayConvention::Unadjusted,
        false,
    )
    .unwrap();
    assert_eq!(future.year(), 2015);
    assert!(
        advance(
            &calendar,
            end,
            3,
            TimeUnit::Months,
            BusinessDayConvention::Following,
            false
        )
        .is_err()
    );
    assert!(
        advance(
            &calendar,
            end,
            3,
            TimeUnit::Months,
            BusinessDayConvention::Following,
            true
        )
        .is_err()
    );
    assert!(
        advance(
            &calendar,
            Date::max_date(),
            1,
            TimeUnit::Months,
            BusinessDayConvention::Unadjusted,
            false
        )
        .is_err()
    );
    let last = crate::time_api::date(40_908).unwrap();
    assert_eq!(
        advance(
            &calendar,
            last,
            1,
            TimeUnit::Days,
            BusinessDayConvention::Unadjusted,
            false
        )
        .unwrap()
        .year(),
        2012
    );
    assert!(
        advance(
            &calendar,
            end,
            10,
            TimeUnit::Days,
            BusinessDayConvention::Unadjusted,
            false
        )
        .is_err()
    );
    assert_eq!(
        adjust(&calendar, future, BusinessDayConvention::Unadjusted).unwrap(),
        future
    );
    assert!(adjust(&calendar, future, BusinessDayConvention::Following).is_err());
}
