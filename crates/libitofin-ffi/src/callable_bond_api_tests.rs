use super::*;
use crate::bond_analytics_api::{
    ItofinBondYieldAnalyticsConfig, itofin_bond_clean_price_from_yield,
};
use crate::bond_api::{
    itofin_bond_action, itofin_bond_integer, itofin_bond_real, itofin_bond_set_engine,
    itofin_discounting_bond_engine_new,
};
use libitofin::errors::QlResult;
use libitofin::instruments::{BondResults, CallableBondArguments};
use libitofin::patterns::observable::{AsObservable, Observable, Observer};
use libitofin::pricingengine::{Arguments, GenericEngine, PricingEngine, Results};
use libitofin::shared::{Shared, SharedMut};
use std::ptr::{null, null_mut};

fn context() -> (Context, u64, u64, u64, u64) {
    let mut c = Context::new();
    let settings = shared(libitofin::settings::Settings::new());
    settings.set_evaluation_date(date(45_306).unwrap());
    let settings = c.insert(settings).unwrap();
    let calendar = crate::calendar_api::NativeCalendar {
        inner: libitofin::time::calendars::NullCalendar::new(),
        horizon: None,
        first_year: 1901,
    };
    let dc = c
        .insert(libitofin::time::daycounters::actual365fixed::Actual365Fixed::new())
        .unwrap();
    let schedule = libitofin::time::schedule::Schedule::from_dates(vec![
        date(45_306).unwrap(),
        date(45_488).unwrap(),
        date(45_672).unwrap(),
    ]);
    let schedule = c.insert_with_metadata(schedule, calendar.clone()).unwrap();
    let calendar = c.insert(calendar).unwrap();
    (c, settings, calendar, dc, schedule)
}
fn common(settings: u64, calendar: u64) -> ItofinBondConfig {
    ItofinBondConfig {
        settlement_days: 2,
        face_amount: 1000.0,
        redemption: 100.0,
        settings,
        issue_date: 45_306,
        payment_convention: 2,
        calendar,
    }
}
fn fixed(settings: u64, dc: u64, schedule: u64) -> ItofinFixedBondConfig {
    ItofinFixedBondConfig {
        bond: common(settings, 0),
        schedule,
        day_counter: dc,
        first_period_day_counter: 0,
        ex_coupon_calendar: 0,
        ex_coupon_length: 0,
        ex_coupon_unit: 0,
        ex_coupon_convention: 2,
        has_ex_coupon: 0,
        ex_coupon_end_of_month: 0,
    }
}
fn call() -> ItofinCallability {
    ItofinCallability {
        price: 100.0,
        price_type: 0,
        call_type: 0,
        date: 45_488,
    }
}
fn make(c: &mut Context, settings: u64, calendar: u64, dc: u64, schedule: u64, zero: bool) -> u64 {
    let mut id = 0;
    let status = unsafe {
        if zero {
            itofin_callable_zero_coupon_bond_new(
                c,
                common(settings, calendar),
                45_672,
                dc,
                &call(),
                1,
                &mut id,
                null_mut(),
            )
        } else {
            itofin_callable_fixed_rate_bond_new(
                c,
                fixed(settings, dc, schedule),
                [0.05].as_ptr(),
                1,
                &call(),
                1,
                &mut id,
                null_mut(),
            )
        }
    };
    assert_eq!(status, 0);
    id
}
struct CountingEngine {
    base: GenericEngine<CallableBondArguments, BondResults>,
    calls: Shared<Cell<usize>>,
}
impl AsObservable for CountingEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}
impl PricingEngine for CountingEngine {
    fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }
    fn results(&self) -> &dyn Results {
        self.base.results()
    }
    fn reset(&mut self) {
        self.base.reset();
    }
    fn calculate(&mut self) -> QlResult<()> {
        assert_eq!(
            self.base.arguments().callability_dates,
            vec![date(45_488).unwrap()]
        );
        let count = self.calls.get() + 1;
        self.calls.set(count);
        self.base.results_mut().instrument.value = Some(900.0 + count as f64);
        self.base.results_mut().settlement_value = Some(910.0 + count as f64);
        Ok(())
    }
}
struct Notifications(Shared<Cell<usize>>);
impl Observer for Notifications {
    fn update(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn callable_dispatch_shares_actual_typed_lazy_state_and_silent_zero_flags() {
    for zero in [false, true] {
        let (mut c, settings_id, calendar, dc, schedule) = context();
        let id = make(&mut c, settings_id, calendar, dc, schedule, zero);
        let native = c.get::<NativeBond>(id).unwrap();
        let alias = native.clone();
        let calls = shared(Cell::new(0));
        let engine = shared_mut(CountingEngine {
            base: GenericEngine::new(CallableBondArguments::default(), BondResults::default()),
            calls: calls.clone(),
        });
        native
            .bond
            .with_instrument_mut(|bond| bond.base_mut().set_pricing_engine(engine.clone()));
        let observed = shared(Cell::new(0));
        let observer: SharedMut<dyn Observer> = shared_mut(Notifications(observed.clone()));
        native
            .bond
            .with_instrument(|bond| bond.base().register_observer(&observer));
        unsafe {
            let mut value = 0.0;
            assert_eq!(
                itofin_bond_real(&mut c, id, 0, 0, &mut value, null_mut()),
                0
            );
            assert_eq!(value, 901.0);
            assert_eq!(alias.bond.real(1, date(45_308).unwrap()).unwrap(), 911.0);
            assert_eq!(calls.get(), 1);
            settings(&c, settings_id)
                .unwrap()
                .set_include_reference_date_events(true);
            let mut calculated = -1;
            assert_eq!(
                itofin_bond_integer(&mut c, id, 5, 0, &mut calculated, null_mut()),
                0
            );
            assert_eq!(calculated, 0);
            assert_eq!(observed.get(), usize::from(!zero));
            assert_eq!(itofin_bond_action(&mut c, id, 0, null_mut()), 0);
            assert_eq!(calls.get(), 2);
            observed.set(0);
            assert_eq!(itofin_bond_action(&mut c, id, 1, null_mut()), 0);
            settings(&c, settings_id)
                .unwrap()
                .set_include_reference_date_events(false);
            assert_eq!(
                itofin_bond_integer(&mut c, id, 5, 0, &mut calculated, null_mut()),
                0
            );
            assert_eq!(calculated, 0);
            assert_eq!(observed.get(), 0);
            assert_eq!(
                itofin_bond_real(&mut c, id, 0, 0, &mut value, null_mut()),
                0
            );
            assert_eq!(value, 902.0);
            assert_eq!(calls.get(), 2);
            assert_eq!(itofin_bond_action(&mut c, id, 0, null_mut()), 0);
            assert_eq!(calls.get(), 3);
            assert_eq!(
                itofin_bond_set_engine(&mut c, id, dc, null_mut()),
                INVALID_HANDLE
            );
            let mut discount = 0;
            let mut straight_engine = 0;
            assert_eq!(
                crate::curves_api::itofin_flat_forward_new(
                    &mut c,
                    45_306,
                    0.04,
                    dc,
                    &mut discount,
                    null_mut()
                ),
                0
            );
            assert_eq!(
                itofin_discounting_bond_engine_new(
                    &mut c,
                    discount,
                    settings_id,
                    -1,
                    &mut straight_engine,
                    null_mut()
                ),
                0
            );
            assert_eq!(
                itofin_bond_set_engine(&mut c, id, straight_engine, null_mut()),
                INVALID_ARGUMENT
            );
            let erased: SharedMut<dyn PricingEngine> = engine.clone();
            let installed = native
                .bond
                .with_instrument(|bond| bond.base().pricing_engine().unwrap().clone());
            assert!(SharedMut::ptr_eq(&installed, &erased));
            let a = ItofinBondYieldAnalyticsConfig {
                day_counter: dc,
                compounding: 1,
                frequency: 1,
                settlement: 45_308,
            };
            assert_eq!(
                itofin_bond_clean_price_from_yield(&mut c, id, 0.04, a, &mut value, null_mut()),
                0
            );
            assert_eq!(calls.get(), 3);
            assert!(
                alias
                    .bond
                    .with_instrument(|bond| bond.base().is_calculated())
            );
            assert_eq!(itofin_bond_action(&mut c, id, 2, null_mut()), 0);
            assert_eq!(itofin_handle_release(&mut c, id, null_mut()), 0);
            assert_eq!(alias.bond.real(0, date(45_308).unwrap()).unwrap(), 903.0);
        }
    }
}
#[test]
fn callable_values_schedule_and_failed_outputs_are_checked() {
    let (mut c, settings_id, calendar, dc, schedule) = context();
    let mut untouched = 991;
    unsafe {
        for bad in [
            ItofinCallability {
                price: f64::NAN,
                ..call()
            },
            ItofinCallability {
                price: -1.0,
                ..call()
            },
            ItofinCallability {
                price_type: 2,
                ..call()
            },
            ItofinCallability {
                call_type: 2,
                ..call()
            },
            ItofinCallability { date: 0, ..call() },
        ] {
            assert_ne!(
                itofin_callable_fixed_rate_bond_new(
                    &mut c,
                    fixed(settings_id, dc, schedule),
                    [0.05].as_ptr(),
                    1,
                    &bad,
                    1,
                    &mut untouched,
                    null_mut()
                ),
                0
            );
            assert_eq!(untouched, 991);
        }
        assert_eq!(
            itofin_callable_zero_coupon_bond_new(
                &mut c,
                common(settings_id, calendar),
                45_672,
                dc,
                null(),
                1,
                &mut untouched,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        let mut unsupported = fixed(settings_id, dc, schedule);
        unsupported.first_period_day_counter = dc;
        assert_eq!(
            itofin_callable_fixed_rate_bond_new(
                &mut c,
                unsupported,
                [0.05].as_ptr(),
                1,
                null(),
                0,
                &mut untouched,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        let calls = [
            call(),
            ItofinCallability {
                call_type: 1,
                price: 0.0,
                price_type: 1,
                ..call()
            },
            ItofinCallability {
                date: 45_306,
                ..call()
            },
        ];
        let mut id = 0;
        assert_eq!(
            itofin_callable_fixed_rate_bond_new(
                &mut c,
                fixed(settings_id, dc, schedule),
                [0.05].as_ptr(),
                1,
                calls.as_ptr(),
                calls.len(),
                &mut id,
                null_mut()
            ),
            0
        );
        let mut count = 991;
        let mut records = [call(); 3];
        assert_eq!(
            itofin_callable_bond_callability(
                &mut c,
                id,
                records.as_mut_ptr(),
                1,
                &mut count,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(count, 991);
        assert_eq!(
            itofin_callable_bond_callability(
                &mut c,
                id,
                records.as_mut_ptr(),
                3,
                &mut count,
                null_mut()
            ),
            0
        );
        assert_eq!(count, 3);
        for (got, want) in records.iter().zip(&calls) {
            assert_eq!(got.price, want.price);
            assert_eq!(got.call_type, want.call_type);
            assert_eq!(got.date, want.date);
            assert_eq!(got.price_type, want.price_type);
        }
        let mut config = common(settings_id, calendar);
        config.redemption = 0.0;
        assert_eq!(
            itofin_callable_zero_coupon_bond_new(
                &mut c,
                config,
                45_672,
                dc,
                null(),
                0,
                &mut id,
                null_mut()
            ),
            0
        );
        let mut value = -1.0;
        assert_eq!(
            itofin_bond_real(&mut c, id, 0, 0, &mut value, null_mut()),
            CORE_ERROR
        );
        assert_eq!(value, -1.0);
        settings(&c, settings_id)
            .unwrap()
            .set_evaluation_date(date(45_672).unwrap());
        assert_eq!(
            itofin_bond_real(&mut c, id, 3, 0, &mut value, null_mut()),
            0
        );
        assert_eq!(value, 0.0);
    }
}

#[test]
fn callable_calendar_math_and_null_ex_coupon_default_preserve_guards() {
    let (mut c, settings_id, _, dc, _) = context();
    let maturity = Date::new(15, libitofin::time::date::Month::January, 2045);
    let mut calendar = 0;
    unsafe {
        assert_eq!(
            crate::calendar_api::itofin_calendar_country_new(
                &mut c,
                25,
                0,
                &mut calendar,
                null_mut()
            ),
            0
        );
    }
    let retained = c
        .get::<crate::calendar_api::NativeCalendar>(calendar)
        .unwrap();
    assert!(retained.checked(maturity).is_err());
    let schedule = libitofin::time::schedule::MakeSchedule::new()
        .from(date(45_306).unwrap())
        .to(maturity)
        .with_frequency(Frequency::Semiannual)
        .with_calendar(retained.inner.clone())
        .with_convention(BusinessDayConvention::Unadjusted)
        .with_termination_date_convention(BusinessDayConvention::Unadjusted)
        .build();
    let schedule = c.insert_with_metadata(schedule, retained).unwrap();
    let mut id = 0;
    unsafe {
        assert_eq!(
            itofin_callable_zero_coupon_bond_new(
                &mut c,
                common(settings_id, calendar),
                maturity.serial_number(),
                dc,
                null(),
                0,
                &mut id,
                null_mut()
            ),
            0
        );
        let mut adjusted = common(settings_id, calendar);
        adjusted.payment_convention = 1;
        let mut untouched = 991;
        assert_eq!(
            itofin_callable_zero_coupon_bond_new(
                &mut c,
                adjusted,
                maturity.serial_number(),
                dc,
                null(),
                0,
                &mut untouched,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(untouched, 991);
        let mut config = fixed(settings_id, dc, schedule);
        config.has_ex_coupon = 1;
        config.ex_coupon_length = 7;
        assert_eq!(
            itofin_callable_fixed_rate_bond_new(
                &mut c,
                config,
                [0.05].as_ptr(),
                1,
                null(),
                0,
                &mut id,
                null_mut()
            ),
            0
        );
        let flows = c
            .get::<NativeBond>(id)
            .unwrap()
            .bond
            .borrow()
            .cashflows()
            .clone();
        assert_eq!(flows[0].ex_coupon_date(), Some(flows[0].date() - 7));
        let mut config = fixed(settings_id, dc, schedule);
        config.has_ex_coupon = 1;
        config.ex_coupon_length = 7;
        config.ex_coupon_calendar = calendar;
        assert_eq!(
            itofin_callable_fixed_rate_bond_new(
                &mut c,
                config,
                [0.05].as_ptr(),
                1,
                null(),
                0,
                &mut untouched,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(untouched, 991);
        let mut config = fixed(settings_id, dc, schedule);
        config.has_ex_coupon = 1;
        config.ex_coupon_length = 1;
        config.ex_coupon_unit = 2;
        config.ex_coupon_end_of_month = 1;
        config.ex_coupon_calendar = calendar;
        assert_eq!(
            itofin_callable_fixed_rate_bond_new(
                &mut c,
                config,
                [0.05].as_ptr(),
                1,
                null(),
                0,
                &mut id,
                null_mut()
            ),
            0
        );
    }
}

#[test]
fn callable_native_once_frequency_is_retained_without_narrowing_construction() {
    let (mut c, settings_id, calendar, dc, _) = context();
    let mut schedule = 0;
    let mut id = 0;
    unsafe {
        assert_eq!(
            crate::time_api::itofin_schedule_new(
                &mut c,
                crate::time_api::ItofinScheduleConfig {
                    start: 45_306,
                    end: 45_672,
                    frequency: 0,
                    calendar,
                    convention: 2,
                    rule: 2,
                    termination_convention: 2,
                },
                &mut schedule,
                null_mut()
            ),
            0
        );
        assert_eq!(
            itofin_callable_fixed_rate_bond_new(
                &mut c,
                fixed(settings_id, dc, schedule),
                [0.05].as_ptr(),
                1,
                null(),
                0,
                &mut id,
                null_mut()
            ),
            0
        );
        assert_eq!(
            c.get::<NativeBond>(id).unwrap().fixed.unwrap().0,
            Frequency::Once
        );
        let mut ordinal = 991;
        assert_eq!(
            itofin_bond_integer(&mut c, id, 6, 0, &mut ordinal, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(ordinal, 991);
        let mut value = -1.0;
        assert_eq!(
            itofin_bond_clean_price_from_yield(
                &mut c,
                id,
                0.04,
                ItofinBondYieldAnalyticsConfig {
                    day_counter: dc,
                    compounding: 1,
                    frequency: 1,
                    settlement: 45_308
                },
                &mut value,
                null_mut()
            ),
            0
        );
        assert!(value.is_finite() && value > 0.0);
    }
}
