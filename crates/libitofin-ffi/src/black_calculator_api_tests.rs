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
