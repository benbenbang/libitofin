use super::*;
use crate::bond_api::{
    ItofinBondConfig, ItofinFixedBondConfig, itofin_bond_action, itofin_bond_real,
    itofin_zero_coupon_bond_new,
};
use crate::callable_bond_api::{
    ItofinCallability, itofin_callable_fixed_rate_bond_new, itofin_callable_zero_coupon_bond_new,
};
use crate::curves_api::{itofin_flat_forward_new, itofin_node_curve_new};
use crate::market_api::{itofin_quote_new, itofin_quote_set};
use crate::settings_api::itofin_settings_new;
use crate::time_api::{
    ItofinScheduleConfig, date, itofin_calendar_new, itofin_day_counter_new, itofin_schedule_new,
};
use libitofin::instrument::Instrument;
use libitofin::pricingengine::PricingEngine;
use serde_json::Value;
use std::ptr::null_mut;

fn number(row: &Value, name: &str) -> f64 {
    row[name].as_f64().unwrap()
}
fn integer(row: &Value, name: &str) -> i32 {
    i32::try_from(row[name].as_i64().unwrap()).unwrap()
}
fn common(row: &Value, settings: u64, calendar: u64) -> ItofinBondConfig {
    ItofinBondConfig {
        settlement_days: row["settlement_days"].as_u64().unwrap() as u32,
        face_amount: number(row, "face"),
        redemption: number(row, "redemption"),
        settings,
        issue_date: integer(row, "issue"),
        payment_convention: integer(row, "payment_convention"),
        calendar,
    }
}
fn fixed(row: &Value, settings: u64, schedule: u64, dc: u64) -> ItofinFixedBondConfig {
    ItofinFixedBondConfig {
        bond: common(row, settings, 0),
        schedule,
        day_counter: dc,
        first_period_day_counter: 0,
        ex_coupon_calendar: 0,
        ex_coupon_length: integer(row, "ex_length"),
        ex_coupon_unit: integer(row, "ex_unit"),
        ex_coupon_convention: 2,
        has_ex_coupon: u8::from(integer(row, "ex_length") != 0),
        ex_coupon_end_of_month: u8::from(row["ex_eom"].as_bool().unwrap()),
    }
}
fn rows() -> Vec<Value> {
    let data: Value = serde_json::from_str(include_str!(
        "../../../sdk/go/testdata/callable_black_oracle.json"
    ))
    .unwrap();
    assert_eq!(data["quantlib_version"], "1.43");
    let rows = data["cases"].as_array().unwrap().clone();
    assert_eq!(rows.len(), 59);
    let mut qualified: Vec<_> = rows
        .iter()
        .filter(|r| r["iv_known_vol_unidentifiable"].as_bool() == Some(true))
        .map(|r| r["name"].as_str().unwrap().to_owned())
        .collect();
    qualified.sort();
    let mut wanted = Vec::new();
    let mut strict = Vec::new();
    for direction in ["call", "put"] {
        for lag in [0, 3, 15] {
            wanted.push(format!("zero_{direction}_lag{lag}_vol0.1"));
            strict.push(format!("zero_{direction}_lag{lag}_vol0.1_strike85"));
        }
    }
    wanted.sort();
    strict.sort();
    assert_eq!(qualified, wanted);
    let mut actual_strict: Vec<_> = rows
        .iter()
        .filter(|r| r["name"].as_str().unwrap().ends_with("_strike85"))
        .map(|r| {
            assert_eq!(r["strike"], 85);
            assert_eq!(r["volatility"], 0.1);
            assert_ne!(r["iv_known_vol_unidentifiable"].as_bool(), Some(true));
            r["name"].as_str().unwrap().to_owned()
        })
        .collect();
    actual_strict.sort();
    assert_eq!(actual_strict, strict);
    rows
}
fn fixture(row: &Value) -> (Context, u64, u64, u64, u64, u64) {
    let mut c = Context::new();
    let (mut settings_id, mut calendar, mut dc, mut curve, mut model, mut engine, mut bond) =
        (0, 0, 0, 0, 0, 0, 0);
    unsafe {
        assert_eq!(itofin_settings_new(&mut c, &mut settings_id, null_mut()), 0);
        settings(&c, settings_id)
            .unwrap()
            .set_evaluation_date(date(integer(row, "evaluation")).unwrap());
        assert_eq!(
            itofin_calendar_new(
                &mut c,
                if row["calendar"] == "null" { 1 } else { 0 },
                &mut calendar,
                null_mut()
            ),
            0
        );
        assert_eq!(
            itofin_day_counter_new(&mut c, integer(row, "day_counter"), &mut dc, null_mut()),
            0
        );
        let mut curve_dc = 0;
        assert_eq!(
            itofin_day_counter_new(&mut c, 1, &mut curve_dc, null_mut()),
            0
        );
        let dates: Vec<i32> = row["curve_dates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_i64().unwrap() as i32)
            .collect();
        let rates: Vec<f64> = row["curve_rates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_f64().unwrap())
            .collect();
        if dates.is_empty() {
            assert_eq!(
                itofin_flat_forward_new(
                    &mut c,
                    integer(row, "reference"),
                    number(row, "rate"),
                    curve_dc,
                    &mut curve,
                    null_mut()
                ),
                0
            );
        } else {
            assert_eq!(
                itofin_node_curve_new(
                    &mut c,
                    0,
                    dates.as_ptr(),
                    rates.as_ptr(),
                    dates.len(),
                    dc,
                    0,
                    &mut curve,
                    null_mut()
                ),
                0
            );
        }
        assert_eq!(
            itofin_quote_new(&mut c, number(row, "volatility"), &mut model, null_mut()),
            0
        );
        assert_eq!(
            itofin_black_callable_bond_engine_new(
                &mut c,
                model,
                curve,
                settings_id,
                &mut engine,
                null_mut()
            ),
            0
        );
        let calls = [ItofinCallability {
            price: number(row, "strike"),
            price_type: integer(row, "price_type"),
            call_type: i32::from(row["put"].as_bool().unwrap()),
            date: integer(row, "exercise"),
        }];
        if row["kind"] == "zero" {
            assert_eq!(
                itofin_callable_zero_coupon_bond_new(
                    &mut c,
                    common(row, settings_id, calendar),
                    integer(row, "maturity"),
                    dc,
                    calls.as_ptr(),
                    calls.len(),
                    &mut bond,
                    null_mut()
                ),
                0
            );
        } else {
            let mut schedule = 0;
            assert_eq!(
                itofin_schedule_new(
                    &mut c,
                    ItofinScheduleConfig {
                        start: integer(row, "start"),
                        end: integer(row, "maturity"),
                        frequency: integer(row, "frequency"),
                        calendar,
                        convention: 2,
                        rule: integer(row, "schedule_rule"),
                        termination_convention: 2
                    },
                    &mut schedule,
                    null_mut()
                ),
                0
            );
            let coupons: Vec<f64> = row["coupons"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_f64().unwrap())
                .collect();
            assert_eq!(
                itofin_callable_fixed_rate_bond_new(
                    &mut c,
                    fixed(row, settings_id, schedule, dc),
                    coupons.as_ptr(),
                    coupons.len(),
                    calls.as_ptr(),
                    calls.len(),
                    &mut bond,
                    null_mut()
                ),
                0
            );
            assert_eq!(itofin_handle_release(&mut c, schedule, null_mut()), 0);
        }
        assert_eq!(
            itofin_callable_bond_set_black_engine(&mut c, bond, engine, null_mut()),
            0
        );
        assert_eq!(itofin_handle_release(&mut c, calendar, null_mut()), 0);
    }
    (c, bond, engine, model, curve, settings_id)
}
fn real(c: &mut Context, id: u64, field: i32) -> f64 {
    let mut out = -999.0;
    assert_eq!(
        unsafe { itofin_bond_real(c, id, field, 0, &mut out, null_mut()) },
        0
    );
    out
}
fn close(got: f64, expected: f64, tolerance: f64) {
    assert!(
        got.is_finite() && (got - expected).abs() <= tolerance,
        "{got:.17} != {expected:.17}, tolerance {tolerance}"
    );
}

fn defaults() -> ItofinCallableImpliedVolatilityConfig {
    ItofinCallableImpliedVolatilityConfig {
        flags: 0,
        accuracy: f64::NAN,
        max_evaluations: 0,
        min_vol: f64::NAN,
        max_vol: f64::NAN,
    }
}
fn iv(
    c: &mut Context,
    id: u64,
    target: f64,
    kind: i32,
    discount: u64,
    a: ItofinCallableImpliedVolatilityConfig,
) -> (i32, f64, ItofinError) {
    let mut out = -991.;
    let mut error = ItofinError {
        code: 0,
        message: [0; 1024],
    };
    let code = unsafe {
        itofin_callable_bond_implied_volatility(
            c, id, target, kind, discount, a, &mut out, &mut error,
        )
    };
    assert_eq!(code, error.code);
    if code != 0 {
        assert_eq!(out, -991.);
        assert_ne!(error.message[0], 0);
    }
    (code, out, error)
}
#[test]
fn callable_black_independent_prices_and_iv_endpoints() {
    for row in rows() {
        let (mut c, id, _, quote, discount, _) = fixture(&row);
        let expected = &row["expected"];
        for (field, name) in [
            (0, "npv"),
            (1, "settlement_value"),
            (2, "clean_price"),
            (3, "dirty_price"),
        ] {
            let actual = real(&mut c, id, field);
            let want = number(expected, name);
            if field <= 1 && number(&row, "face") == 1000. {
                close(actual, want, 1e-8);
            } else if field <= 1 {
                close(
                    actual / number(&row, "face") * 100.,
                    want / number(&row, "face") * 100.,
                    1e-9,
                );
            } else {
                close(actual, want, 1e-9);
            }
        }
        for (kind, name) in [(0, "clean_price"), (1, "dirty_price")] {
            let want = if !row["lower_endpoint"].is_null()
                && expected[name] == row["lower_endpoint"][name]
            {
                0.
            } else {
                number(&row, "volatility")
            };
            let (target, price_kind) = if want == 0. {
                assert_eq!(
                    unsafe { itofin_quote_set(&mut c, quote, 0., null_mut()) },
                    0
                );
                (real(&mut c, id, 3), 1)
            } else {
                (number(expected, name), kind)
            };
            let (code, root, error) = iv(&mut c, id, target, price_kind, discount, defaults());
            assert_eq!(code, 0, "{} {:?}", row["name"], unsafe {
                std::ffi::CStr::from_ptr(error.message.as_ptr())
            });
            if row["iv_known_vol_unidentifiable"].as_bool() == Some(true) {
                assert!(root.is_finite() && (0. ..=1.).contains(&root));
                assert_eq!(
                    unsafe { itofin_quote_set(&mut c, quote, root, null_mut()) },
                    0
                );
                close(real(&mut c, id, 2), number(expected, "clean_price"), 1e-9);
                close(real(&mut c, id, 3), number(expected, "dirty_price"), 1e-9);
            } else {
                assert!(
                    (root - want).abs() <= 2e-8,
                    "case {} kind {} root {:.17} want {:.17}",
                    row["name"],
                    kind,
                    root,
                    want
                );
            }
            assert_eq!(
                unsafe { itofin_quote_set(&mut c, quote, number(&row, "volatility"), null_mut()) },
                0
            );
        }
        if !row["lower_endpoint"].is_null() {
            let mut a = defaults();
            a.flags = 2;
            a.max_evaluations = 1;
            assert_eq!(
                unsafe { itofin_quote_set(&mut c, quote, 0., null_mut()) },
                0
            );
            let target = real(&mut c, id, 3);
            close(target, number(&row["lower_endpoint"], "dirty_price"), 1e-9);
            let (code, root, error) = iv(&mut c, id, target, 1, discount, a);
            assert_eq!(code, 0, "{} {:?}", row["name"], unsafe {
                std::ffi::CStr::from_ptr(error.message.as_ptr())
            });
            assert_eq!(root, 0.);
            if row["upper_endpoint"]["dirty_price"] != row["lower_endpoint"]["dirty_price"] {
                assert_eq!(
                    unsafe { itofin_quote_set(&mut c, quote, 1., null_mut()) },
                    0
                );
                let target = real(&mut c, id, 3);
                close(target, number(&row["upper_endpoint"], "dirty_price"), 1e-9);
                a.max_evaluations = 2;
                let (code, root, error) = iv(&mut c, id, target, 1, discount, a);
                assert_eq!(code, 0, "{} {:?}", row["name"], unsafe {
                    std::ffi::CStr::from_ptr(error.message.as_ptr())
                });
                assert_eq!(root, 1.);
            }
        }
    }
}
#[derive(Clone)]
struct CountObserver(std::rc::Rc<std::cell::Cell<usize>>);
impl libitofin::patterns::observable::Observer for CountObserver {
    fn update(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
#[derive(Debug, PartialEq)]
struct EngineSnapshot {
    value: Option<f64>,
    valuation_date: Option<Date>,
    settlement_value: Option<f64>,
    dates: Vec<Date>,
    strikes: Vec<f64>,
    count: usize,
}
fn engine_snapshot(c: &Context, engine: u64) -> EngineSnapshot {
    use libitofin::instruments::BondResults;
    use libitofin::pricingengines::bond::BlackCallableBondArguments;
    let e = c.get::<NativeCallableBlackEngine>(engine).unwrap();
    let mut e = e.engine.borrow_mut();
    let args = (e.arguments_mut() as &mut dyn std::any::Any)
        .downcast_mut::<BlackCallableBondArguments>()
        .unwrap();
    let dates = args.bond.callability_dates.clone();
    let strikes = args.bond.callability_prices.clone();
    let count = args.callability_count;
    let result = (e.results() as &dyn std::any::Any)
        .downcast_ref::<BondResults>()
        .unwrap();
    EngineSnapshot {
        value: result.instrument.value,
        valuation_date: result.instrument.valuation_date,
        settlement_value: result.settlement_value,
        dates,
        strikes,
        count,
    }
}
#[test]
fn callable_black_iv_preserves_actual_engine_cache_observers_and_pending_flags() {
    for kind in ["fixed", "zero"] {
        let row = rows()
            .into_iter()
            .find(|row| row["name"] == format!("{kind}_call_lag3_vol0.35"))
            .unwrap();
        let (mut c, id, engine, quote, discount, settings_id) = fixture(&row);
        let cached = real(&mut c, id, 0);
        let target = real(&mut c, id, 2);
        let native = c.get::<NativeBond>(id).unwrap();
        let installed = native
            .bond
            .borrow()
            .base()
            .pricing_engine()
            .unwrap()
            .clone();
        let state = engine_snapshot(&c, engine);
        let flags = native.flags.get();
        let notifications = std::rc::Rc::new(std::cell::Cell::new(0));
        let observer: SharedMut<dyn libitofin::patterns::observable::Observer> =
            shared_mut(CountObserver(notifications.clone()));
        native.bond.borrow().base().register_observer(&observer);
        assert_eq!(unsafe { itofin_bond_action(&mut c, id, 1, null_mut()) }, 0);
        settings(&c, settings_id)
            .unwrap()
            .set_include_reference_date_events(true);
        settings(&c, settings_id)
            .unwrap()
            .set_include_todays_cash_flows(Some(true));
        let mut limited = defaults();
        limited.flags = 2;
        limited.max_evaluations = 1;
        let mut bad = defaults();
        bad.flags = 16;
        for (target, a, want) in [
            (target, defaults(), 0),
            (target, limited, CORE_ERROR),
            (10000., defaults(), CORE_ERROR),
            (-1000., defaults(), CORE_ERROR),
            (-0.01, defaults(), CORE_ERROR),
            (target, bad, INVALID_ARGUMENT),
        ] {
            assert_eq!(iv(&mut c, id, target, 0, discount, a).0, want);
            assert!(native.bond.borrow().base().is_calculated());
            assert_eq!(native.bond.borrow().base().results().value, Some(cached));
            assert_eq!(state, engine_snapshot(&c, engine));
            assert_eq!(native.flags.get(), flags);
            assert_eq!(notifications.get(), 0);
            assert!(SharedMut::ptr_eq(
                native.bond.borrow().base().pricing_engine().unwrap(),
                &installed
            ));
        }
        assert_eq!(
            unsafe { itofin_quote_set(&mut c, quote, 0.42, null_mut()) },
            0
        );
        close(
            native.bond.with_instrument_mut(|b| b.npv()).unwrap(),
            cached,
            0.,
        );
        assert_eq!(unsafe { itofin_bond_action(&mut c, id, 2, null_mut()) }, 0);
        assert_ne!(real(&mut c, id, 0), cached);
    }
}
#[test]
fn callable_black_native_guards_and_sentinels() {
    let row = rows()
        .into_iter()
        .find(|r| r["name"] == "fixed_call_lag3_vol0.35")
        .unwrap();
    let (mut c, id, engine, quote, discount, settings_id) = fixture(&row);
    let cached = real(&mut c, id, 0);
    let (mut other, mut wrong) = (0, 0);
    unsafe {
        assert_eq!(itofin_settings_new(&mut c, &mut other, null_mut()), 0);
        assert_eq!(
            itofin_black_callable_bond_engine_new(
                &mut c,
                quote,
                discount,
                other,
                &mut wrong,
                null_mut()
            ),
            0
        );
        assert_eq!(
            itofin_callable_bond_set_black_engine(&mut c, id, wrong, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_callable_bond_set_black_engine(&mut c, id, quote, null_mut()),
            INVALID_HANDLE
        );
    }
    assert_eq!(real(&mut c, id, 0), cached);
    for (kind, a, want) in [
        (99, defaults(), INVALID_ARGUMENT),
        (
            0,
            ItofinCallableImpliedVolatilityConfig {
                flags: 1,
                accuracy: 0.,
                ..defaults()
            },
            INVALID_ARGUMENT,
        ),
        (
            0,
            ItofinCallableImpliedVolatilityConfig {
                flags: 2,
                max_evaluations: 0,
                ..defaults()
            },
            INVALID_ARGUMENT,
        ),
        (
            0,
            ItofinCallableImpliedVolatilityConfig {
                flags: 4,
                min_vol: -1.,
                ..defaults()
            },
            INVALID_ARGUMENT,
        ),
        (
            0,
            ItofinCallableImpliedVolatilityConfig {
                flags: 8,
                max_vol: 0.,
                ..defaults()
            },
            INVALID_ARGUMENT,
        ),
        (
            0,
            ItofinCallableImpliedVolatilityConfig {
                flags: 32,
                ..defaults()
            },
            INVALID_ARGUMENT,
        ),
        (
            0,
            ItofinCallableImpliedVolatilityConfig {
                flags: 2,
                max_evaluations: 1,
                ..defaults()
            },
            CORE_ERROR,
        ),
    ] {
        assert_eq!(
            iv(
                &mut c,
                id,
                number(&row["expected"], "clean_price"),
                kind,
                discount,
                a
            )
            .0,
            want
        );
    }
    assert_eq!(
        iv(&mut c, id, f64::NAN, 0, discount, defaults()).0,
        INVALID_ARGUMENT
    );
    assert_eq!(iv(&mut c, id, 100., 0, quote, defaults()).0, INVALID_HANDLE);
    let mut untouched = 991;
    unsafe {
        assert_eq!(
            itofin_black_callable_bond_engine_new(
                &mut c,
                quote,
                discount,
                settings_id,
                null_mut(),
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_black_callable_bond_engine_new(
                &mut c,
                discount,
                discount,
                settings_id,
                &mut untouched,
                null_mut()
            ),
            INVALID_HANDLE
        );
    }
    assert_eq!(untouched, 991);
    unsafe {
        assert_eq!(
            itofin_callable_bond_implied_volatility(
                &mut c,
                id,
                100.,
                0,
                discount,
                defaults(),
                null_mut(),
                null_mut()
            ),
            INVALID_ARGUMENT
        );
    }
    let mut calendar = 0;
    let mut plain = 0;
    unsafe {
        assert_eq!(itofin_calendar_new(&mut c, 1, &mut calendar, null_mut()), 0);
        assert_eq!(
            itofin_zero_coupon_bond_new(
                &mut c,
                common(&row, settings_id, calendar),
                integer(&row, "maturity"),
                &mut plain,
                null_mut()
            ),
            0
        );
        assert_eq!(
            itofin_callable_bond_set_black_engine(&mut c, plain, engine, null_mut()),
            INVALID_ARGUMENT
        );
    }
    assert_eq!(
        iv(&mut c, plain, 100., 0, discount, defaults()).0,
        INVALID_ARGUMENT
    );
    settings(&c, settings_id)
        .unwrap()
        .set_evaluation_date(date(integer(&row, "maturity") + 1).unwrap());
    assert_eq!(real(&mut c, id, 0), 0.);
    assert_eq!(iv(&mut c, id, 100., 0, discount, defaults()).0, CORE_ERROR);
    unsafe {
        assert_eq!(itofin_handle_release(&mut c, engine, null_mut()), 0);
        assert_eq!(
            itofin_callable_bond_set_black_engine(&mut c, id, engine, null_mut()),
            INVALID_HANDLE
        );
    }
    let ptr = &mut c as *mut Context as usize;
    let code = std::thread::spawn(move || {
        let mut out = 991;
        let code = unsafe {
            itofin_black_callable_bond_engine_new(
                ptr as *mut Context,
                quote,
                discount,
                settings_id,
                &mut out,
                null_mut(),
            )
        };
        assert_eq!(out, 991);
        code
    })
    .join()
    .unwrap();
    assert_eq!(code, WRONG_THREAD);
}
#[test]
fn callable_black_live_relink_and_final_owner_release() {
    use libitofin::handle::RelinkableHandle;
    use libitofin::interestrate::Compounding;
    use libitofin::quotes::SimpleQuote;
    use libitofin::shared::shared;
    use libitofin::termstructures::yields::FlatForward;
    use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
    use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
    use libitofin::time::frequency::Frequency;
    let row = rows()
        .into_iter()
        .find(|r| r["name"] == "fixed_call_lag3_vol0.1")
        .unwrap();
    let (mut c, id, old_engine, vol_id, old_curve, settings_id) = fixture(&row);
    let make_curve = |rate| -> Shared<dyn YieldTermStructure> {
        shared(FlatForward::with_rate(
            date(integer(&row, "reference")).unwrap(),
            rate,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        ))
    };
    let link = RelinkableHandle::new(make_curve(0.04));
    let curve_id = c.insert(link.handle()).unwrap();
    let mut engine_id = 0;
    unsafe {
        assert_eq!(
            itofin_black_callable_bond_engine_new(
                &mut c,
                vol_id,
                curve_id,
                settings_id,
                &mut engine_id,
                null_mut()
            ),
            0
        );
        assert_eq!(
            itofin_callable_bond_set_black_engine(&mut c, id, engine_id, null_mut()),
            0
        );
    }
    close(real(&mut c, id, 0), number(&row["expected"], "npv"), 1e-8);
    unsafe {
        assert_eq!(itofin_quote_set(&mut c, vol_id, 0.42, null_mut()), 0);
    }
    close(
        real(&mut c, id, 0),
        number(&row["updated_volatility"], "npv"),
        1e-8,
    );
    link.reset();
    assert_eq!(iv(&mut c, id, 100., 0, curve_id, defaults()).0, CORE_ERROR);
    let mut untouched = -991.;
    assert_eq!(
        unsafe { itofin_bond_real(&mut c, id, 0, 0, &mut untouched, null_mut()) },
        CORE_ERROR
    );
    assert_eq!(untouched, -991.);
    link.link_to(make_curve(0.051));
    close(
        real(&mut c, id, 0),
        number(&row["updated_rate"], "npv"),
        1e-8,
    );
    let quote_weak = Shared::downgrade(&c.get::<Shared<SimpleQuote>>(vol_id).unwrap());
    let curve_weak = Shared::downgrade(&link.handle().current_link().unwrap());
    let settings_weak = Shared::downgrade(&settings(&c, settings_id).unwrap());
    let engine_weak = SharedMut::downgrade(
        &c.get::<NativeCallableBlackEngine>(engine_id)
            .unwrap()
            .engine,
    );
    drop(link);
    unsafe {
        for h in [
            old_engine,
            vol_id,
            old_curve,
            curve_id,
            engine_id,
            settings_id,
        ] {
            assert_eq!(itofin_handle_release(&mut c, h, null_mut()), 0);
        }
        assert_eq!(itofin_bond_action(&mut c, id, 0, null_mut()), 0);
    }
    assert!(quote_weak.upgrade().is_some());
    assert!(curve_weak.upgrade().is_some());
    assert!(settings_weak.upgrade().is_some());
    assert!(engine_weak.upgrade().is_some());
    unsafe {
        assert_eq!(itofin_handle_release(&mut c, id, null_mut()), 0);
    }
    assert!(quote_weak.upgrade().is_none());
    assert!(curve_weak.upgrade().is_none());
    assert!(settings_weak.upgrade().is_none());
    assert!(engine_weak.upgrade().is_none());
}
