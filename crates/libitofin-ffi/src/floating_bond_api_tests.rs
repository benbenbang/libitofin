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
