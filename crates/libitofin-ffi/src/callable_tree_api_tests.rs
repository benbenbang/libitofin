use super::*;
use crate::bond_api::{
    ItofinBondConfig, ItofinFixedBondConfig, itofin_bond_action, itofin_bond_integer,
    itofin_bond_real, itofin_zero_coupon_bond_new,
};
use crate::callable_bond_api::{
    ItofinCallability, itofin_callable_fixed_rate_bond_new, itofin_callable_zero_coupon_bond_new,
};
use crate::curves_api::{itofin_flat_forward_new, itofin_node_curve_new};
use crate::models_api::itofin_hullwhite_new;
use crate::settings_api::itofin_settings_new;
use crate::time_api::{
    ItofinScheduleConfig, date, itofin_calendar_new, itofin_day_counter_new, itofin_schedule_new,
};
use libitofin::instrument::Instrument;
use libitofin::models::model::CalibratedModelHolder;
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
        "../../../sdk/go/testdata/callable_tree_oracle.json"
    ))
    .unwrap();
    assert_eq!(data["quantlib_version"], "1.43");
    data["cases"].as_array().unwrap().clone()
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
        assert_eq!(itofin_day_counter_new(&mut c, 1, &mut dc, null_mut()), 0);
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
                    dc,
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
            itofin_hullwhite_new(
                &mut c,
                curve,
                number(row, "a"),
                number(row, "sigma"),
                &mut model,
                null_mut()
            ),
            0
        );
        assert_eq!(
            itofin_tree_callable_bond_engine_new(
                &mut c,
                model,
                row["steps"].as_u64().unwrap(),
                settings_id,
                &mut engine,
                null_mut()
            ),
            0
        );
        let calls: Vec<ItofinCallability> = row["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|event| ItofinCallability {
                price: event[0].as_f64().unwrap(),
                price_type: event[1].as_i64().unwrap() as i32,
                call_type: event[2].as_i64().unwrap() as i32,
                date: event[3].as_i64().unwrap() as i32,
            })
            .collect();
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
                        frequency: 1,
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
            itofin_callable_bond_set_tree_engine(&mut c, bond, engine, null_mut()),
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

#[test]
fn callable_tree_native_independent_oracles_and_retained_graphs() {
    for row in rows() {
        let (mut c, id, engine, model, curve, settings_id) = fixture(&row);
        let expected = &row["expected"];
        for (field, name) in [
            (0, "npv"),
            (1, "settlement_value"),
            (2, "clean"),
            (3, "dirty"),
            (4, "accrued"),
        ] {
            close(
                real(&mut c, id, field),
                number(expected, name),
                if field < 2 {
                    number(&row, "currency_tolerance")
                } else if field == 4 {
                    1e-14
                } else {
                    number(&row, "price_tolerance")
                },
            );
        }
        let native = c.get::<NativeBond>(id).unwrap();
        assert!(
            native
                .bond
                .with_instrument(|bond| bond.base().is_calculated())
        );
        close(
            real(&mut c, id, 1),
            real(&mut c, id, 0) / number(expected, "settlement_discount"),
            number(&row, "currency_tolerance"),
        );
        if row["name"] == "zero_zero_redemption_put" {
            assert!(real(&mut c, id, 0) > 0.0);
        }
        unsafe {
            for handle in [engine, model, curve, settings_id] {
                assert_eq!(itofin_handle_release(&mut c, handle, null_mut()), 0);
            }
            assert_eq!(itofin_bond_action(&mut c, id, 0, null_mut()), 0);
        }
        close(
            real(&mut c, id, 2),
            number(expected, "clean"),
            number(&row, "price_tolerance"),
        );
    }
}
#[test]
fn callable_tree_attachment_guards_preserve_existing_cache_and_pending_zero_flags() {
    for kind in ["fixed", "zero"] {
        let row = rows()
            .into_iter()
            .find(|row| {
                row["name"]
                    == if kind == "fixed" {
                        "fixed_call_clean"
                    } else {
                        "zero_call_clean"
                    }
            })
            .unwrap();
        let (mut c, id, engine, model, _, settings_id) = fixture(&row);
        let cached = real(&mut c, id, 0);
        let native = c.get::<NativeBond>(id).unwrap();
        let installed = native
            .bond
            .borrow()
            .base()
            .pricing_engine()
            .unwrap()
            .clone();
        let (mut other_settings, mut wrong_engine) = (0, 0);
        unsafe {
            assert_eq!(
                itofin_settings_new(&mut c, &mut other_settings, null_mut()),
                0
            );
            settings(&c, other_settings)
                .unwrap()
                .set_evaluation_date(date(integer(&row, "evaluation")).unwrap());
            assert_eq!(
                itofin_tree_callable_bond_engine_new(
                    &mut c,
                    model,
                    100,
                    other_settings,
                    &mut wrong_engine,
                    null_mut()
                ),
                0
            );
            assert_eq!(itofin_bond_action(&mut c, id, 1, null_mut()), 0);
            settings(&c, settings_id)
                .unwrap()
                .set_include_reference_date_events(true);
            assert_eq!(
                itofin_callable_bond_set_tree_engine(&mut c, id, wrong_engine, null_mut()),
                INVALID_ARGUMENT
            );
            assert!(native.bond.borrow().base().is_calculated());
            assert!(SharedMut::ptr_eq(
                native.bond.borrow().base().pricing_engine().unwrap(),
                &installed
            ));
            assert_eq!(
                native.bond.with_instrument_mut(|bond| bond.npv()).unwrap(),
                cached
            );
            assert_eq!(
                itofin_callable_bond_set_tree_engine(&mut c, id, model, null_mut()),
                INVALID_HANDLE
            );
            assert_eq!(itofin_handle_release(&mut c, wrong_engine, null_mut()), 0);
            assert_eq!(
                itofin_callable_bond_set_tree_engine(&mut c, id, wrong_engine, null_mut()),
                INVALID_HANDLE
            );
            let mut dc = 0;
            let mut calendar = 0;
            assert_eq!(itofin_day_counter_new(&mut c, 1, &mut dc, null_mut()), 0);
            assert_eq!(itofin_calendar_new(&mut c, 0, &mut calendar, null_mut()), 0);
            let mut plain = 0;
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
                itofin_callable_bond_set_tree_engine(&mut c, plain, engine, null_mut()),
                INVALID_ARGUMENT
            );
            let mut untouched = 991;
            assert_eq!(
                itofin_tree_callable_bond_engine_new(
                    &mut c,
                    model,
                    0,
                    settings_id,
                    &mut untouched,
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(untouched, 991);
            assert_eq!(
                itofin_tree_callable_bond_engine_new(
                    &mut c,
                    model,
                    100,
                    settings_id,
                    null_mut(),
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_tree_callable_bond_engine_new(
                    &mut c,
                    dc,
                    100,
                    settings_id,
                    &mut untouched,
                    null_mut()
                ),
                INVALID_HANDLE
            );
            assert_eq!(untouched, 991);
            assert_eq!(
                itofin_callable_bond_set_tree_engine(&mut Context::new(), id, engine, null_mut()),
                INVALID_HANDLE
            );
            settings(&c, settings_id)
                .unwrap()
                .set_include_reference_date_events(false);
            assert_eq!(itofin_bond_action(&mut c, id, 2, null_mut()), 0);
        }
        let address = (&mut c as *mut Context) as usize;
        assert_eq!(
            std::thread::spawn(move || unsafe {
                itofin_callable_bond_set_tree_engine(
                    address as *mut Context,
                    id,
                    engine,
                    null_mut(),
                )
            })
            .join()
            .unwrap(),
            WRONG_THREAD
        );
        assert!(real(&mut c, id, 2).is_finite());
    }
}
#[test]
fn callable_tree_actual_model_update_and_replacement_use_native_observers() {
    let row = rows()
        .into_iter()
        .find(|row| row["name"] == "fixed_call_clean")
        .unwrap();
    let (mut c, id, engine, model_id, _, settings_id) = fixture(&row);
    let initial = real(&mut c, id, 0);
    let model = c.get::<SharedMut<HullWhite>>(model_id).unwrap();
    model
        .borrow_mut()
        .set_params(&libitofin::math::array::Array::from([0.073, 0.014]))
        .unwrap();
    let native = c.get::<NativeBond>(id).unwrap();
    assert!(!native.bond.borrow().base().is_calculated());
    let changed = real(&mut c, id, 0);
    assert_ne!(initial, changed);
    let mut replacement = 0;
    unsafe {
        assert_eq!(
            itofin_tree_callable_bond_engine_new(
                &mut c,
                model_id,
                100,
                settings_id,
                &mut replacement,
                null_mut()
            ),
            0
        );
        assert_eq!(
            itofin_callable_bond_set_tree_engine(&mut c, id, replacement, null_mut()),
            0
        );
    }
    close(real(&mut c, id, 0), changed, 1e-10);
    let installed = c.get::<NativeCallableTreeEngine>(replacement).unwrap();
    let erased: SharedMut<dyn PricingEngine> = installed.engine.clone();
    assert!(SharedMut::ptr_eq(
        native.bond.borrow().base().pricing_engine().unwrap(),
        &erased
    ));
    unsafe {
        assert_eq!(itofin_handle_release(&mut c, engine, null_mut()), 0);
    }
    let mut calculated = 0;
    assert_eq!(
        unsafe { itofin_bond_integer(&mut c, id, 5, 0, &mut calculated, null_mut()) },
        0
    );
    assert_eq!(calculated, 1);
}

#[test]
fn callable_tree_native_relink_reset_and_last_owner_release_preserve_actual_graph() {
    use libitofin::handle::RelinkableHandle;
    use libitofin::interestrate::Compounding;
    use libitofin::shared::shared;
    use libitofin::termstructures::yields::FlatForward;
    use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
    use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
    use libitofin::time::frequency::Frequency;
    let row = rows()
        .into_iter()
        .find(|row| row["name"] == "fixed_call_clean")
        .unwrap();
    let (mut c, id, old_engine, old_model, old_curve, settings_id) = fixture(&row);
    let make_curve = |rate| -> Shared<dyn YieldTermStructure> {
        shared(FlatForward::with_rate(
            date(integer(&row, "reference")).unwrap(),
            rate,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        ))
    };
    let link = RelinkableHandle::new(make_curve(0.03));
    let model: SharedMut<HullWhite> = HullWhite::new(link.handle(), 0.1, 0.01).unwrap();
    let model_weak = SharedMut::downgrade(&model);
    let model_id = c.insert(model.clone()).unwrap();
    let wrong_model_id = c.insert(shared_mut(model.clone())).unwrap();
    let mut error = ItofinError {
        code: 0,
        message: [0; 1024],
    };
    let mut untouched_engine = 791;
    assert_eq!(
        unsafe {
            itofin_tree_callable_bond_engine_new(
                &mut c,
                wrong_model_id,
                100,
                settings_id,
                &mut untouched_engine,
                &mut error,
            )
        },
        INVALID_HANDLE
    );
    assert_eq!(error.code, INVALID_HANDLE);
    assert_eq!(
        unsafe { std::ffi::CStr::from_ptr(error.message.as_ptr()) }
            .to_str()
            .unwrap(),
        "unknown, released, foreign, or wrong-type handle"
    );
    assert_eq!(untouched_engine, 791);
    assert_eq!(
        unsafe { itofin_handle_release(&mut c, wrong_model_id, null_mut()) },
        0
    );
    let direct = TreeCallableFixedRateBondEngine::new(
        model.clone(),
        100,
        settings(&c, settings_id).unwrap(),
    )
    .unwrap();
    let mut engine_id = 0;
    unsafe {
        assert_eq!(
            itofin_tree_callable_bond_engine_new(
                &mut c,
                model_id,
                100,
                settings_id,
                &mut engine_id,
                &mut error
            ),
            0,
            "{:?}",
            std::ffi::CStr::from_ptr(error.message.as_ptr())
        );
        assert_eq!(
            itofin_callable_bond_set_tree_engine(&mut c, id, engine_id, null_mut()),
            0
        );
    }
    close(real(&mut c, id, 0), number(&row["expected"], "npv"), 1e-10);
    link.reset();
    assert!(direct.discount_curve().is_err());
    drop(direct);
    let mut untouched = -991.0;
    assert_eq!(
        unsafe { itofin_bond_real(&mut c, id, 0, 0, &mut untouched, &mut error) },
        CORE_ERROR,
        "{:?}",
        unsafe { std::ffi::CStr::from_ptr(error.message.as_ptr()) }
    );
    assert_eq!(error.code, CORE_ERROR);
    assert_eq!(untouched, -991.0);
    link.link_to(make_curve(0.045));
    close(
        real(&mut c, id, 0),
        number(&row["updated_rate"], "npv"),
        1e-10,
    );
    let engine_weak =
        SharedMut::downgrade(&c.get::<NativeCallableTreeEngine>(engine_id).unwrap().engine);
    let curve_weak = Shared::downgrade(&link.handle().current_link().unwrap());
    let settings_weak = Shared::downgrade(&settings(&c, settings_id).unwrap());
    drop(model);
    drop(link);
    unsafe {
        for handle in [
            old_engine,
            old_model,
            old_curve,
            model_id,
            engine_id,
            settings_id,
        ] {
            assert_eq!(itofin_handle_release(&mut c, handle, null_mut()), 0);
        }
        assert_eq!(itofin_bond_action(&mut c, id, 0, null_mut()), 0);
    }
    assert!(model_weak.upgrade().is_some());
    assert!(engine_weak.upgrade().is_some());
    assert!(curve_weak.upgrade().is_some());
    assert!(settings_weak.upgrade().is_some());
    assert_eq!(unsafe { itofin_handle_release(&mut c, id, null_mut()) }, 0);
    assert!(model_weak.upgrade().is_none());
    assert!(engine_weak.upgrade().is_none());
    assert!(curve_weak.upgrade().is_none());
    assert!(settings_weak.upgrade().is_none());
}
