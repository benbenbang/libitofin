use super::*;
use std::ptr::null_mut;

#[test]
fn immutable_native_scalar_domain_and_pointer_contracts() {
    let mut c = Context::new();
    let mut id = 999;
    assert_eq!(
        unsafe { itofin_black_calculator_new(&mut c, 0, 0., 100., 0.2, 0.95, &mut id, null_mut()) },
        0
    );
    let mut out = 999.;
    for query in [
        itofin_black_calculator_gamma_forward,
        itofin_black_calculator_strike_sensitivity,
    ] {
        assert_eq!(
            unsafe { query(&mut c, id, &mut out, null_mut()) },
            INVALID_ARGUMENT
        );
        assert_eq!(out, 999.);
    }
    assert_eq!(
        unsafe { itofin_black_calculator_value(&mut c, id, &mut out, null_mut()) },
        0
    );
    assert_eq!(out, 95.);
    out = 999.;
    assert_eq!(
        unsafe { itofin_black_calculator_value(&mut c, id, null_mut(), null_mut()) },
        INVALID_ARGUMENT
    );
    let bad_out = std::ptr::dangling_mut::<u8>().cast::<f64>();
    assert_eq!(
        unsafe { itofin_black_calculator_value(&mut c, id, bad_out, null_mut()) },
        INVALID_ARGUMENT
    );
    let bad_error = std::ptr::dangling_mut::<u8>().cast::<ItofinError>();
    assert_eq!(
        unsafe { itofin_black_calculator_value(&mut c, id, &mut out, bad_error) },
        INVALID_ARGUMENT
    );
    assert_eq!(out, 999.);
    let address = (&mut c as *mut Context) as usize;
    std::thread::spawn(move || {
        let mut out = 999.;
        assert_eq!(
            unsafe {
                itofin_black_calculator_value(address as *mut Context, id, &mut out, null_mut())
            },
            WRONG_THREAD
        );
        assert_eq!(out, 999.);
    })
    .join()
    .unwrap();
    assert_eq!(
        unsafe { crate::boundary::itofin_handle_release(&mut c, id, null_mut()) },
        0
    );
    assert_eq!(
        unsafe { itofin_black_calculator_value(&mut c, id, &mut out, null_mut()) },
        INVALID_HANDLE
    );
    assert_eq!(out, 999.);
}

#[test]
fn constructors_validate_before_writing_handle() {
    let mut c = Context::new();
    let mut id = 999;
    for parameter in 0..4 {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.] {
            let mut args = [100., 110., 0.2, 0.95];
            args[parameter] = bad;
            assert_eq!(
                unsafe {
                    itofin_black_calculator_new(
                        &mut c,
                        0,
                        args[0],
                        args[1],
                        args[2],
                        args[3],
                        &mut id,
                        null_mut(),
                    )
                },
                CORE_ERROR
            );
            assert_eq!(id, 999);
        }
    }
    assert_eq!(
        unsafe {
            itofin_black_calculator_new(&mut c, -1, 100., 110., 0.2, 0.95, &mut id, null_mut())
        },
        INVALID_ARGUMENT
    );
    assert_eq!(id, 999);
}

#[test]
fn cash_digital_domain_fence_and_unchanged_outputs() {
    let mut c = Context::new();
    let mut id = 999;
    for parameter in 0..5 {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut args = [100., 7., 110., 0.2, 0.95];
            args[parameter] = bad;
            assert_ne!(
                unsafe {
                    itofin_black_calculator_cash_digital_new(
                        &mut c,
                        0,
                        args[0],
                        args[1],
                        args[2],
                        args[3],
                        args[4],
                        &mut id,
                        null_mut(),
                    )
                },
                0
            );
            assert_eq!(id, 999);
        }
    }
    for deviation in [-1., 0., 1e-20, f64::EPSILON] {
        assert_eq!(
            unsafe {
                itofin_black_calculator_cash_digital_new(
                    &mut c,
                    0,
                    100.,
                    7.,
                    110.,
                    deviation,
                    0.95,
                    &mut id,
                    null_mut(),
                )
            },
            INVALID_ARGUMENT
        );
        assert_eq!(id, 999);
    }
    for option in [-1, 2] {
        assert_eq!(
            unsafe {
                itofin_black_calculator_cash_digital_new(
                    &mut c,
                    option,
                    100.,
                    7.,
                    110.,
                    0.2,
                    0.95,
                    &mut id,
                    null_mut(),
                )
            },
            INVALID_ARGUMENT
        );
        assert_eq!(id, 999);
    }
    let bad_error = std::ptr::dangling_mut::<u8>().cast::<ItofinError>();
    assert_eq!(
        unsafe {
            itofin_black_calculator_cash_digital_new(
                &mut c, 0, 100., 7., 110., 0.2, 0.95, &mut id, bad_error,
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(id, 999);
    for out in [null_mut(), std::ptr::dangling_mut::<u8>().cast::<u64>()] {
        assert_eq!(
            unsafe {
                itofin_black_calculator_cash_digital_new(
                    &mut c,
                    0,
                    100.,
                    7.,
                    110.,
                    0.2,
                    0.95,
                    out,
                    null_mut(),
                )
            },
            INVALID_ARGUMENT
        );
    }
}

#[test]
fn cash_digital_reuses_immutable_queries_and_typed_contexts() {
    let mut c = Context::new();
    let mut other = Context::new();
    for option in [0, 1] {
        for cash in [-7., 0., 13.] {
            let mut id = 0;
            assert_eq!(
                unsafe {
                    itofin_black_calculator_cash_digital_new(
                        &mut c,
                        option,
                        100.,
                        cash,
                        110.,
                        0.2,
                        0.95,
                        &mut id,
                        null_mut(),
                    )
                },
                0
            );
            let payoff = CashOrNothingPayoff::new(option_type(option).unwrap(), 100., cash);
            let core = BlackCalculator::with_striked_payoff(&payoff, 110., 0.2, 0.95).unwrap();
            let mut out = 999.;
            assert_eq!(
                unsafe { itofin_black_calculator_value(&mut c, id, &mut out, null_mut()) },
                0
            );
            assert_eq!(out, core.value());
            assert_eq!(
                unsafe {
                    itofin_black_calculator_elasticity_forward(&mut c, id, &mut out, null_mut())
                },
                0
            );
            assert_eq!(out, core.elasticity_forward());
            out = 999.;
            assert_eq!(
                unsafe { itofin_black_calculator_value(&mut other, id, &mut out, null_mut()) },
                INVALID_HANDLE
            );
            assert_eq!(out, 999.);
            assert_eq!(
                unsafe { crate::boundary::itofin_handle_release(&mut c, id, null_mut()) },
                0
            );
            assert_eq!(
                unsafe { itofin_black_calculator_value(&mut c, id, &mut out, null_mut()) },
                INVALID_HANDLE
            );
            assert_eq!(out, 999.);
        }
    }
    let mut id = 0;
    assert_eq!(
        unsafe {
            itofin_black_calculator_cash_digital_new(
                &mut c,
                0,
                0.,
                7.,
                110.,
                0.2,
                0.95,
                &mut id,
                null_mut(),
            )
        },
        0
    );
    let mut out = 999.;
    assert_eq!(
        unsafe { itofin_black_calculator_gamma_forward(&mut c, id, &mut out, null_mut()) },
        INVALID_ARGUMENT
    );
    assert_eq!(out, 999.);
    assert_eq!(
        unsafe { itofin_black_calculator_value(&mut c, id, &mut out, null_mut()) },
        0
    );
    assert!((out - 6.65).abs() < 1e-14);
}
