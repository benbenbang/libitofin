use crate::boundary::*;
use crate::curves_api::itofin_curve_value;
use crate::joint_curves_api::itofin_flat_forward_from_quote;
use crate::market_api::{itofin_quote_new, itofin_quote_set, itofin_quote_value};
use crate::quotes_api::*;
use libitofin::time::daycounters::actual360::Actual360;
use std::ptr::null_mut;

type Affine = (f64, f64, fn(f64) -> f64);
type Op = (i32, fn(f64, f64) -> f64);

pub(crate) fn error() -> ItofinError {
    ItofinError {
        code: 0,
        message: [0; 1024],
    }
}

fn message(e: &ItofinError) -> String {
    unsafe { std::ffi::CStr::from_ptr(e.message.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

pub(crate) fn simple(c: &mut Context, value: f64) -> u64 {
    let mut out = 0;
    assert_eq!(
        unsafe { itofin_quote_new(c, value, &mut out, null_mut()) },
        0
    );
    out
}

fn set(c: &mut Context, id: u64, value: f64) {
    assert_eq!(unsafe { itofin_quote_set(c, id, value, null_mut()) }, 0);
}

pub(crate) fn value(c: &mut Context, id: u64) -> f64 {
    let mut out = 0.0;
    assert_eq!(
        unsafe { itofin_quote_value(c, id, &mut out, null_mut()) },
        0
    );
    out
}

fn derived(c: &mut Context, source: u64, a: f64, b: f64) -> u64 {
    let mut out = 0;
    let code = unsafe { itofin_derived_quote_new(c, source, a, b, &mut out, null_mut()) };
    assert_eq!(code, 0);
    out
}

fn composite(c: &mut Context, q1: u64, q2: u64, op: i32) -> u64 {
    let mut out = 0;
    let code = unsafe { itofin_composite_quote_new(c, q1, q2, op, &mut out, null_mut()) };
    assert_eq!(code, 0);
    out
}

fn discount(c: &mut Context, curve: u64, t: f64) -> f64 {
    let mut out = 0.0;
    let code = unsafe { itofin_curve_value(c, curve, 0, t, 0.0, 0, false, &mut out, null_mut()) };
    assert_eq!(code, 0);
    out
}

/// Port of `testDerived` (test-suite/quotes.cpp:97-115): add10, mul10 and
/// sub10 as `a * x + b`.
#[test]
fn derived_quote_matches_quantlib_functions() {
    let mut c = Context::new();
    let me = simple(&mut c, 0.0);
    let funcs: [Affine; 3] = [
        (1.0, 10.0, |x| x + 10.0),
        (10.0, 0.0, |x| x * 10.0),
        (1.0, -10.0, |x| x - 10.0),
    ];
    for (a, b, f) in funcs {
        let d = derived(&mut c, me, a, b);
        for v in [12.0, 23.0, 34.0] {
            set(&mut c, me, v);
            assert!((value(&mut c, d) - f(v)).abs() <= 1e-10);
        }
    }
}

/// Port of `testComposite` (test-suite/quotes.cpp:121-140).
#[test]
fn composite_quote_matches_quantlib_functions() {
    let mut c = Context::new();
    let me1 = simple(&mut c, 0.0);
    let me2 = simple(&mut c, 0.0);
    let funcs: [Op; 3] = [
        (ITOFIN_COMPOSITE_ADD, |x, y| x + y),
        (ITOFIN_COMPOSITE_MULTIPLY, |x, y| x * y),
        (ITOFIN_COMPOSITE_SUBTRACT, |x, y| x - y),
    ];
    for (op, f) in funcs {
        let q = composite(&mut c, me1, me2, op);
        for v in [12.0, 23.0, 34.0] {
            set(&mut c, me1, v);
            set(&mut c, me2, v + 1.0);
            assert!((value(&mut c, q) - f(v, v + 1.0)).abs() <= 1e-10);
        }
    }
}

#[test]
fn chained_dependents_and_square_track_the_source() {
    let mut c = Context::new();
    let x = simple(&mut c, 0.75);
    let square = composite(&mut c, x, x, ITOFIN_COMPOSITE_MULTIPLY);
    let triple = derived(&mut c, x, 3.0, 0.0);
    let chained = derived(&mut c, triple, 2.0, 1.0);
    let mixed = composite(&mut c, square, chained, ITOFIN_COMPOSITE_ADD);
    assert_eq!(value(&mut c, square), 0.5625);
    assert_eq!(value(&mut c, chained), 5.5);
    assert_eq!(value(&mut c, mixed), 6.0625);
    set(&mut c, x, 2.0);
    assert_eq!(value(&mut c, square), 4.0);
    assert_eq!(value(&mut c, chained), 13.0);
    assert_eq!(value(&mut c, mixed), 17.0);
}

#[test]
fn flat_forward_on_a_dependent_quote_reprices_and_outlives_released_handles() {
    let mut c = Context::new();
    let dc = c.insert(Actual360::new()).unwrap();
    let base = simple(&mut c, 0.01);
    let rate = derived(&mut c, base, 2.0, 0.0);
    let mut curve = 0;
    let code =
        unsafe { itofin_flat_forward_from_quote(&mut c, 45000, rate, dc, &mut curve, null_mut()) };
    assert_eq!(code, 0);
    assert!((discount(&mut c, curve, 2.0) - (-0.04_f64).exp()).abs() <= 1e-15);
    assert_eq!(
        unsafe { itofin_handle_release(&mut c, rate, null_mut()) },
        0
    );
    set(&mut c, base, 0.02);
    assert!((discount(&mut c, curve, 2.0) - (-0.08_f64).exp()).abs() <= 1e-15);
}

#[test]
fn unknown_operation_and_non_quote_handles_are_errors() {
    let mut c = Context::new();
    let dc = c.insert(Actual360::new()).unwrap();
    let q = simple(&mut c, 1.0);
    let mut out = 0;
    let mut e = error();
    let code = unsafe { itofin_composite_quote_new(&mut c, q, q, 3, &mut out, &mut e) };
    assert_eq!(code, INVALID_ARGUMENT);
    assert_eq!(message(&e), "unknown composite quote operation");
    let mut e = error();
    let code = unsafe { itofin_derived_quote_new(&mut c, dc, 1.0, 0.0, &mut out, &mut e) };
    assert_eq!(code, INVALID_HANDLE);
    let mut e = error();
    let code = unsafe { itofin_composite_quote_new(&mut c, q, dc, 0, &mut out, &mut e) };
    assert_eq!(code, INVALID_HANDLE);
    let mut v = 0.0;
    let mut e = error();
    assert_eq!(
        unsafe { itofin_quote_value(&mut c, dc, &mut v, &mut e) },
        INVALID_HANDLE
    );
    let d = derived(&mut c, q, 2.0, 0.0);
    let mut e = error();
    assert_eq!(
        unsafe { itofin_quote_set(&mut c, d, 5.0, &mut e) },
        INVALID_HANDLE
    );
    assert_eq!(value(&mut c, d), 2.0);
}
