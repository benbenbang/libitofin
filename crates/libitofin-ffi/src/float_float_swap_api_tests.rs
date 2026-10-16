use super::*;
#[path = "../../libitofin/tests/fixtures/medium4_floatfloat/support.rs"]
mod support;
use libitofin::shared::shared;
use libitofin::time::calendars::WeekendsOnly;
use libitofin::time::daycounters::{actual360::Actual360, actual365fixed::Actual365Fixed};
use std::ptr::{null, null_mut};
const OK: i32 = 0;

struct Fixture {
    c: Context,
    config: ItofinFloatFloatSwapConfig,
    discount: u64,
    market: support::Market,
}
impl Fixture {
    fn new(row: &[f64]) -> Self {
        let market = support::Market::new(row);
        let mut c = Context::new();
        let calendar = NativeCalendar {
            inner: WeekendsOnly::new(),
            horizon: None,
            first_year: 1901,
        };
        let schedules = [false, true].map(|second| {
            c.insert_with_metadata(support::schedule(second), calendar.clone())
                .unwrap()
        });
        let indexes = market
            .indexes
            .clone()
            .map(|index| c.insert(NativeIbor::builtin(index)).unwrap());
        let settings = c.insert(market.settings.clone()).unwrap();
        let counters = [
            c.insert(Actual360::new()).unwrap(),
            c.insert(Actual365Fixed::new()).unwrap(),
        ];
        let discount = c.insert(market.curves[2].handle()).unwrap();
        let config = ItofinFloatFloatSwapConfig {
            nominal: 1_000_000.,
            pay_schedule: schedules[0],
            pay_index: indexes[0],
            pay_spread: row[8],
            pay_day_counter: counters[0],
            receive_schedule: schedules[1],
            receive_index: indexes[1],
            receive_spread: row[9],
            receive_day_counter: counters[1],
            settings,
            has_payment_convention: u8::from(row[10] != 0.),
            payment_convention: 1,
        };
        Self {
            c,
            config,
            discount,
            market,
        }
    }
    fn build(&mut self) -> u64 {
        let mut id = 0;
        assert_eq!(
            unsafe { itofin_float_float_swap_new(&mut self.c, &self.config, &mut id, null_mut()) },
            OK
        );
        id
    }
    fn attach(&mut self, id: u64) {
        assert_eq!(
            unsafe {
                itofin_float_float_swap_set_engine(
                    &mut self.c,
                    id,
                    self.discount,
                    self.config.settings,
                    null_mut(),
                )
            },
            OK
        );
    }
    fn value(&mut self, id: u64, query: i32) -> f64 {
        let mut result = 999.;
        assert_eq!(
            unsafe {
                itofin_float_float_swap_value(&mut self.c, id, query, &mut result, null_mut())
            },
            OK
        );
        result
    }
}
#[test]
fn float_float_same_profile_quantlib_signed_measures() {
    for (name, row) in support::rows()
        .into_iter()
        .filter(|(name, _)| ["base", "negative", "following", "expired"].contains(name))
    {
        let mut f = Fixture::new(&row);
        let id = f.build();
        f.attach(id);
        for query in 0..=4 {
            support::close(f.value(id, query), row[11 + query as usize], 2e-9);
        }
        for query in 5..=6 {
            if name == "expired" {
                let mut out = 999.;
                assert_ne!(
                    unsafe {
                        itofin_float_float_swap_value(&mut f.c, id, query, &mut out, null_mut())
                    },
                    OK
                );
                assert_eq!(out, 999.);
            } else {
                support::close(f.value(id, query), row[11 + query as usize], 2e-13);
            }
        }
        assert_eq!(f.value(id, 7), 1.);
        assert_eq!(f.value(id, 8), f64::from(u8::from(name == "expired")));
    }
}
#[test]
fn float_float_errors_do_not_insert_or_write_and_identity_does_not_mutate() {
    let mut f = Fixture::new(&support::rows()[0].1);
    let mut out = 987;
    unsafe {
        assert_ne!(
            itofin_float_float_swap_new(&mut f.c, null(), &mut out, null_mut()),
            OK
        );
        assert_eq!(out, 987);
        f.config.has_payment_convention = 2;
        assert_ne!(
            itofin_float_float_swap_new(&mut f.c, &f.config, &mut out, null_mut()),
            OK
        );
        assert_eq!(out, 987);
        f.config.has_payment_convention = 0;
        for nominal in [0., -1., f64::NAN, f64::INFINITY] {
            f.config.nominal = nominal;
            assert_ne!(
                itofin_float_float_swap_new(&mut f.c, &f.config, &mut out, null_mut()),
                OK
            );
            assert_eq!(out, 987);
        }
        f.config.nominal = 1_000_000.;
    }
    let id = f.build();
    let mut value = 999.;
    assert_ne!(
        unsafe { itofin_float_float_swap_value(&mut f.c, id, 0, &mut value, null_mut()) },
        OK
    );
    assert_eq!(value, 999.);
    f.attach(id);
    let before = f.value(id, 0);
    let other = shared(Settings::new());
    other.set_evaluation_date(support::reference());
    let other = f.c.insert(other).unwrap();
    let original_settings = f.config.settings;
    f.config.settings = other;
    assert_ne!(
        unsafe { itofin_float_float_swap_new(&mut f.c, &f.config, &mut out, null_mut()) },
        OK
    );
    assert_eq!(out, 987);
    f.config.settings = original_settings;
    assert_ne!(
        unsafe { itofin_float_float_swap_set_engine(&mut f.c, id, f.discount, other, null_mut()) },
        OK
    );
    assert_eq!(f.value(id, 0), before);
    for query in [-1, 9] {
        assert_ne!(
            unsafe { itofin_float_float_swap_value(&mut f.c, id, query, &mut value, null_mut()) },
            OK
        );
        assert_eq!(value, 999.);
    }
    assert_ne!(
        unsafe { itofin_float_float_swap_value(&mut f.c, id, 0, null_mut(), null_mut()) },
        OK
    );
    assert_ne!(
        unsafe {
            itofin_float_float_swap_value(&mut f.c, f.config.settings, 0, &mut value, null_mut())
        },
        OK
    );
    assert_ne!(
        unsafe { itofin_float_float_swap_action(&mut f.c, id, 4, null_mut()) },
        OK
    );
}
#[test]
fn float_float_live_fixings_quotes_freeze_and_retained_handles() {
    use libitofin::quotes::Quote;
    let mut f = Fixture::new(&support::rows()[0].1);
    let id = f.build();
    f.attach(id);
    let before = f.value(id, 0);
    assert_eq!(
        unsafe { itofin_float_float_swap_action(&mut f.c, id, 2, null_mut()) },
        OK
    );
    f.market.quotes[0].set_value(0.05);
    assert_eq!(f.value(id, 0), before);
    assert_eq!(
        unsafe { itofin_float_float_swap_action(&mut f.c, id, 1, null_mut()) },
        OK
    );
    let after = f.value(id, 0);
    assert!((after - before).abs() > 1.);
    assert_eq!(
        unsafe { itofin_float_float_swap_action(&mut f.c, id, 3, null_mut()) },
        OK
    );
    f.market
        .settings
        .set_evaluation_date(support::dates(false)[0]);
    f.market.settings.set_enforces_todays_historic_fixings(true);
    let mut out = 999.;
    assert_ne!(
        unsafe { itofin_float_float_swap_value(&mut f.c, id, 0, &mut out, null_mut()) },
        OK
    );
    assert_eq!(out, 999.);
    for index in &f.market.indexes {
        index.add_fixing(support::dates(false)[0], 0.03).unwrap();
    }
    assert!(f.value(id, 0).is_finite());
    for handle in [
        f.config.settings,
        f.config.pay_index,
        f.config.receive_index,
        f.config.pay_schedule,
        f.config.receive_schedule,
        f.discount,
    ] {
        assert_eq!(
            unsafe { itofin_handle_release(&mut f.c, handle, null_mut()) },
            OK
        );
    }
    f.market.quotes[2].set_value(0.04);
    assert!(f.value(id, 0).is_finite());
    f.market.settings.set_evaluation_date(support::date(46405.));
    assert_eq!(f.value(id, 0), 0.);
    assert!(f.market.quotes[0].value().unwrap().is_finite());
}

#[test]
fn float_float_context_thread_optional_flag_and_calendar_guards() {
    let mut f = Fixture::new(&support::rows()[0].1);
    let foreign = Context::new();
    let mut foreign = foreign;
    let mut out = 999;
    assert_ne!(
        unsafe { itofin_float_float_swap_new(&mut foreign, &f.config, &mut out, null_mut()) },
        OK
    );
    assert_eq!(out, 999);
    f.config.has_payment_convention = 1;
    f.config.payment_convention = 999;
    assert_ne!(
        unsafe { itofin_float_float_swap_new(&mut f.c, &f.config, &mut out, null_mut()) },
        OK
    );
    assert_eq!(out, 999);
    f.config.has_payment_convention = 0;
    let index = f.c.get::<NativeIbor>(f.config.receive_index).unwrap();
    let mut index = index;
    index.inner = shared(libitofin::indexes::IborIndex::new(
        "GuardedReceive".into(),
        libitofin::time::period::Period::new(6, TimeUnit::Months),
        0,
        libitofin::currency::Currency::eur(),
        index.fixing_calendar.inner.clone(),
        Bdc::Following,
        false,
        Actual360::new(),
        f.market.curves[1].handle(),
        f.market.settings.clone(),
    ));
    index.maturity_calendar.horizon = Some(2026);
    f.config.receive_index = f.c.insert(index).unwrap();
    let id = f.build();
    f.attach(id);
    let mut value = 999.;
    assert_ne!(
        unsafe { itofin_float_float_swap_value(&mut f.c, id, 0, &mut value, null_mut()) },
        OK
    );
    assert_eq!(value, 999.);
    let address = &mut f.c as *mut Context as usize;
    let result = std::thread::spawn(move || {
        let mut out = 999.;
        let code = unsafe {
            itofin_float_float_swap_value(address as *mut Context, id, 0, &mut out, null_mut())
        };
        (code, out)
    })
    .join()
    .unwrap();
    assert_eq!(result, (WRONG_THREAD, 999.));
}

#[test]
fn float_float_never_calculated_freeze_and_silent_flags_delegate_native() {
    let mut f = Fixture::new(&support::rows()[0].1);
    let id = f.build();
    f.attach(id);
    assert_eq!(
        unsafe { itofin_float_float_swap_action(&mut f.c, id, 2, null_mut()) },
        OK
    );
    let mut out = 999.;
    assert_ne!(
        unsafe { itofin_float_float_swap_value(&mut f.c, id, 0, &mut out, null_mut()) },
        OK
    );
    assert_eq!(out, 999.);
    assert_eq!(
        unsafe { itofin_float_float_swap_action(&mut f.c, id, 1, null_mut()) },
        OK
    );
    let before = f.value(id, 0);
    f.market.settings.set_using_at_par_coupons(false);
    assert_eq!(f.value(id, 0), before);
    assert_eq!(
        unsafe { itofin_float_float_swap_action(&mut f.c, id, 3, null_mut()) },
        OK
    );
    let value = f.value(id, 0);
    let native = f.market.standard(&support::rows()[0].1);
    let mut native = native;
    support::close(value, native.npv().unwrap(), 2e-9);
    f.market.curves[2].reset();
    let mut out = 999.;
    assert_ne!(
        unsafe { itofin_float_float_swap_value(&mut f.c, id, 0, &mut out, null_mut()) },
        OK
    );
    assert_eq!(out, 999.);
}

#[test]
fn float_float_all_silent_settings_flags_match_direct_native_companion() {
    use libitofin::interestrate::Compounding;
    use libitofin::termstructures::yields::FlatForward;
    use libitofin::time::frequency::Frequency;
    let row = support::rows()
        .into_iter()
        .find(|(name, _)| *name == "today_in")
        .unwrap()
        .1;
    let mut f = Fixture::new(&row);
    let discount = Handle::new(shared(FlatForward::with_rate(
        support::date(row[0]),
        row[7],
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )) as Shared<dyn YieldTermStructure>);
    f.discount = f.c.insert(discount.clone()).unwrap();
    let id = f.build();
    f.attach(id);
    let mut native = f.market.standard(&row);
    native
        .base_mut()
        .set_pricing_engine(shared_mut(DiscountingSwapEngine::new(
            discount,
            None,
            None,
            None,
            f.market.settings.clone(),
        )));
    for index in &f.market.indexes {
        index.add_fixing(support::date(row[0]), 0.03).unwrap();
    }
    let mut distinct = Vec::new();
    for include in [false, true] {
        for today in [None, Some(false), Some(true)] {
            for at_par in [false, true] {
                for enforce in [false, true] {
                    f.market.settings.set_include_reference_date_events(include);
                    f.market.settings.set_include_todays_cash_flows(today);
                    f.market.settings.set_using_at_par_coupons(at_par);
                    f.market
                        .settings
                        .set_enforces_todays_historic_fixings(enforce);
                    let value = f.value(id, 0);
                    support::close(value, native.npv().unwrap(), 2e-9);
                    distinct.push(value);
                }
            }
        }
    }
    assert!(distinct.iter().any(|v| (*v - distinct[0]).abs() > 1.));
}

#[test]
fn float_float_empty_forwarding_is_constructible_and_live_relink_recovers() {
    let row = &support::rows()[0].1;
    let mut f = Fixture::new(row);
    let original = f.market.curves[0].handle().current_link().unwrap();
    f.market.curves[0].reset();
    let id = f.build();
    f.attach(id);
    let mut out = 999.;
    assert_ne!(
        unsafe { itofin_float_float_swap_value(&mut f.c, id, 0, &mut out, null_mut()) },
        OK
    );
    assert_eq!(out, 999.);
    f.market.curves[0].link_to(original);
    support::close(f.value(id, 0), row[11], 2e-9);
}

#[test]
fn float_float_omitted_payment_convention_preserves_each_schedule() {
    let row = &support::rows()[0].1;
    let mut f = Fixture::new(row);
    let following = Schedule::with_metadata(
        support::dates(false),
        WeekendsOnly::new(),
        Bdc::Following,
        None,
        None,
        None,
        None,
        vec![],
    );
    let calendar = NativeCalendar {
        inner: WeekendsOnly::new(),
        horizon: None,
        first_year: 1901,
    };
    f.config.pay_schedule = f.c.insert_with_metadata(following, calendar).unwrap();
    let mixed = f.build();
    f.attach(mixed);
    f.config.has_payment_convention = 1;
    f.config.payment_convention = 1;
    let both = f.build();
    f.attach(both);
    support::close(f.value(mixed, 1), f.value(both, 1), 2e-9);
    assert!((f.value(mixed, 2) - f.value(both, 2)).abs() > 0.1);
}
