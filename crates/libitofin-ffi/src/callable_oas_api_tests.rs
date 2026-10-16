use super::*;
use crate::calendar_api::NativeCalendar;
use libitofin::instrument::Instrument;
use libitofin::pricingengine::PricingEngine;
use libitofin::shared::{shared, shared_mut};
use libitofin::time::calendars::NullCalendar;
use libitofin::time::daycounters::{actual360::Actual360, actual365fixed::Actual365Fixed};
use std::cell::Cell;
use std::ptr::null_mut;
#[allow(dead_code)]
#[path = "../../libitofin/tests/fixtures/medium6_callable_oas/support.rs"]
mod native;

fn fixture(zero: bool, lag: u32) -> (Context, u64, u64, u64) {
    let mut c = Context::new();
    let s = native::settings();
    let e = NativeCallableTreeEngine {
        engine: shared_mut(native::engine(s.clone())),
        settings: s.clone(),
    };
    let backing = if zero {
        BondBacking::CallableZero(shared_mut(native::zero(lag, s, native::calls())))
    } else {
        BondBacking::CallableFixed(shared_mut(native::fixed(lag, s, native::calls())))
    };
    let b = NativeBond {
        bond: backing,
        calendar: NativeCalendar {
            inner: NullCalendar::new(),
            horizon: None,
            first_year: 1901,
        },
        fixed: None,
        flags: shared(Cell::new((false, None))),
        floating: None,
    };
    let bond = c.insert(b).unwrap();
    let engine = c.insert(e).unwrap();
    let dc = c.insert(Actual365Fixed::new()).unwrap();
    (c, bond, engine, dc)
}
fn config(dc: u64) -> ItofinCallableOASConfig {
    ItofinCallableOASConfig {
        day_counter: dc,
        compounding: 2,
        frequency: 1,
        settlement_flags: 0,
        settlement: 0,
    }
}
fn defaults() -> ItofinAnalyticsSolverConfig {
    ItofinAnalyticsSolverConfig {
        flags: 0,
        accuracy: f64::NAN,
        max_evaluations: 0,
        guess: f64::NAN,
    }
}
#[test]
fn thirty_independent_native_quotes_and_roots() {
    for line in include_str!("../../libitofin/tests/fixtures/medium6_callable_oas/reference.tsv")
        .lines()
        .skip(1)
    {
        let row: Vec<_> = line.split('\t').collect();
        let (mut c, b, e, dc) = fixture(row[0] == "zero", row[1].parse().unwrap());
        let dc = if row[3] == "360" {
            c.insert(Actual360::new()).unwrap()
        } else {
            dc
        };
        let mut a = config(dc);
        a.compounding = [2, 1, 0, 3, 4][row[2].parse::<usize>().unwrap()];
        let spread = row[4].parse().unwrap();
        let expected = row[6].parse().unwrap();
        unsafe {
            let mut price = -991.0;
            assert_eq!(
                itofin_callable_bond_clean_price_oas(
                    &mut c,
                    b,
                    spread,
                    e,
                    a,
                    &mut price,
                    null_mut()
                ),
                0
            );
            native::close(price, expected, 2e-10);
            let mut root = -991.0;
            assert_eq!(
                itofin_callable_bond_oas(
                    &mut c,
                    b,
                    expected,
                    e,
                    a,
                    defaults(),
                    &mut root,
                    null_mut()
                ),
                0
            );
            native::close(root, spread, 2e-10);
        }
    }
}
#[test]
fn immutable_frozen_state_identity_and_failure_sentinels() {
    for zero in [false, true] {
        let (mut c, b, e, dc) = fixture(zero, 2);
        let b_native = c.get::<NativeBond>(b).unwrap();
        let e_native = c.get::<NativeCallableTreeEngine>(e).unwrap();
        b_native
            .bond
            .with_instrument_mut(|b| b.base_mut().set_pricing_engine(e_native.engine.clone()));
        let value = b_native.bond.with_instrument_mut(|b| b.npv().unwrap());
        b_native.bond.with_instrument_mut(|b| b.base_mut().freeze());
        let stored = e_native
            .engine
            .borrow()
            .results()
            .as_instrument_results()
            .unwrap()
            .value;
        let notifications = shared(Cell::new(0));
        let observer = shared_mut(native::UpdateCounter(notifications.clone()));
        b_native
            .bond
            .with_instrument(|b| b.base().register_observer(&(observer.clone() as _)));
        b_native
            .bond
            .borrow()
            .settings()
            .set_include_reference_date_events(true);
        b_native
            .bond
            .borrow()
            .settings()
            .set_include_todays_cash_flows(Some(true));
        let old_flags = b_native.flags.get();
        let foreign = native::settings();
        let other = c
            .insert(NativeCallableTreeEngine {
                engine: shared_mut(native::engine(foreign.clone())),
                settings: foreign,
            })
            .unwrap();
        unsafe {
            let mut price = 991.0;
            assert_eq!(
                itofin_callable_bond_clean_price_oas(
                    &mut c,
                    b,
                    0.015,
                    e,
                    config(dc),
                    &mut price,
                    null_mut()
                ),
                0
            );
            let mut root = 991.0;
            let mut budget = defaults();
            budget.flags = 2;
            budget.max_evaluations = 1;
            assert_ne!(
                itofin_callable_bond_oas(
                    &mut c,
                    b,
                    price,
                    e,
                    config(dc),
                    budget,
                    &mut root,
                    null_mut()
                ),
                0
            );
            assert_eq!(root, 991.0);
            assert_ne!(
                itofin_callable_bond_clean_price_oas(
                    &mut c,
                    b,
                    0.015,
                    other,
                    config(dc),
                    &mut root,
                    null_mut()
                ),
                0
            );
            assert_ne!(
                itofin_callable_bond_clean_price_oas(
                    &mut c,
                    b,
                    0.015,
                    e,
                    config(dc),
                    null_mut(),
                    null_mut()
                ),
                0
            );
            assert_eq!(root, 991.0);
        }
        assert_eq!(b_native.flags.get(), old_flags);
        assert_eq!(notifications.get(), 0);
        assert_eq!(
            e_native
                .engine
                .borrow()
                .results()
                .as_instrument_results()
                .unwrap()
                .value,
            stored
        );
        let underlying = b_native.bond.borrow();
        assert!(underlying.base().is_calculated());
        assert_eq!(underlying.base().results().value, Some(value));
        assert!(Shared::ptr_eq(
            underlying.base().pricing_engine().unwrap(),
            &(e_native.engine.clone() as _)
        ));
    }
}
#[test]
fn malformed_inputs_do_not_write_results() {
    let (mut c, b, e, dc) = fixture(true, 2);
    unsafe {
        let mut out = 991.0;
        for field in 0..8 {
            let mut a = config(dc);
            match field {
                0 => a.settlement_flags = 2,
                1 => {
                    a.settlement_flags = 1;
                    a.settlement = 0;
                }
                2 => a.compounding = -1,
                3 => a.frequency = -1,
                4 => a.day_counter = b,
                _ => {}
            }
            assert_ne!(
                itofin_callable_bond_clean_price_oas(
                    &mut c,
                    if field == 5 { dc } else { b },
                    if field == 6 { f64::NAN } else { 0.01 },
                    if field == 7 { dc } else { e },
                    a,
                    &mut out,
                    null_mut()
                ),
                0
            );
            assert_eq!(out, 991.0);
        }
        for flags in [1, 2, 4, 8] {
            let a = ItofinAnalyticsSolverConfig {
                flags,
                accuracy: 0.0,
                max_evaluations: 0,
                guess: f64::NAN,
            };
            assert_ne!(
                itofin_callable_bond_oas(&mut c, b, 100.0, e, config(dc), a, &mut out, null_mut()),
                0
            );
            assert_eq!(out, 991.0);
        }
        let mut actual = config(dc);
        actual.settlement_flags = 1;
        actual.settlement = native::reference().serial_number();
        c.get::<NativeBond>(b)
            .unwrap()
            .bond
            .borrow()
            .settings()
            .reset_evaluation_date();
        assert_eq!(
            itofin_callable_bond_clean_price_oas(&mut c, b, 0.0, e, actual, &mut out, null_mut()),
            0
        );
        out = 991.0;
        assert_ne!(
            itofin_callable_bond_clean_price_oas(
                &mut c,
                b,
                0.0,
                e,
                config(dc),
                &mut out,
                null_mut()
            ),
            0
        );
        assert_eq!(out, 991.0);
    }
}

#[test]
fn checked_session_thread_release_and_actual_date_paths() {
    let (mut c, b, e, dc) = fixture(true, 10);
    let (foreign, foreign_bond, _, _) = fixture(true, 10);
    let address = &mut c as *mut Context as usize;
    assert_eq!(
        std::thread::spawn(move || {
            let mut out = 991.0;
            let status = unsafe {
                itofin_callable_bond_clean_price_oas(
                    address as *mut Context,
                    b,
                    0.01,
                    e,
                    config(dc),
                    &mut out,
                    null_mut(),
                )
            };
            assert_eq!(out, 991.0);
            status
        })
        .join()
        .unwrap(),
        WRONG_THREAD
    );
    unsafe {
        let mut out = 991.0;
        assert_eq!(
            itofin_callable_bond_clean_price_oas(
                &mut c,
                foreign_bond,
                0.01,
                e,
                config(dc),
                &mut out,
                null_mut()
            ),
            INVALID_HANDLE
        );
        assert_eq!(out, 991.0);
        let mut actual = config(dc);
        actual.settlement_flags = 1;
        actual.settlement = (native::reference() + 4).serial_number();
        assert_eq!(
            itofin_callable_bond_clean_price_oas(&mut c, b, 0.01, e, actual, &mut out, null_mut()),
            0
        );
        let old = out;
        c.get::<NativeBond>(b)
            .unwrap()
            .bond
            .borrow()
            .settings()
            .set_evaluation_date(native::reference() + 8);
        assert_eq!(
            itofin_callable_bond_clean_price_oas(&mut c, b, 0.01, e, actual, &mut out, null_mut()),
            0
        );
        assert_eq!(out.to_bits(), old.to_bits());
        assert_eq!(itofin_handle_release(&mut c, e, null_mut()), 0);
        out = 991.0;
        assert_eq!(
            itofin_callable_bond_clean_price_oas(&mut c, b, 0.01, e, actual, &mut out, null_mut()),
            INVALID_HANDLE
        );
        assert_eq!(out, 991.0);
    }
    drop(foreign);
}
