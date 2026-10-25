use crate::boundary::*;
use crate::scalar_payoff_api::*;
use std::ptr::null_mut;

unsafe fn create(c: &mut Context, kind: i32, option: i32, strike: f64, extra: f64) -> u64 {
    let mut out = 0;
    assert_eq!(
        unsafe { itofin_scalar_payoff_new(c, kind, option, strike, extra, &mut out, null_mut()) },
        0
    );
    out
}

#[test]
fn scalar_payoff_native_boundaries_and_stored_terms() {
    let mut c = Context::new();
    unsafe {
        for option in [0, 1] {
            for (kind, extra, expected) in [
                (0, 0.0, 0.0),
                (1, -7.0, 0.0),
                (2, 0.0, 0.0),
                (3, 110.0, if option == 0 { -10.0 } else { 10.0 }),
            ] {
                let id = create(&mut c, kind, option, 100.0, extra);
                let mut value = -123.0;
                assert_eq!(
                    itofin_scalar_payoff_value(&mut c, id, 100.0, &mut value, null_mut()),
                    0
                );
                assert_eq!(value, expected);
                let mut info = ItofinScalarPayoffInfo {
                    kind: -1,
                    option_kind: -1,
                    strike: 0.0,
                    extra: 0.0,
                };
                assert_eq!(
                    itofin_scalar_payoff_info(&mut c, id, &mut info, null_mut()),
                    0
                );
                assert_eq!(
                    (info.kind, info.option_kind, info.strike, info.extra),
                    (kind, option, 100.0, extra)
                );
            }
        }
        let id = create(&mut c, 4, 0, 0.8, 0.0);
        let mut value = 0.0;
        assert_eq!(
            itofin_scalar_payoff_value(&mut c, id, -100.0, &mut value, null_mut()),
            0
        );
        assert!((value + 20.0).abs() < 1e-12);
        let asset = create(&mut c, 2, 0, -2.0, 0.0);
        assert_eq!(
            itofin_scalar_payoff_value(&mut c, asset, -1.0, &mut value, null_mut()),
            0
        );
        assert_eq!(value, -1.0);
    }
}

#[test]
fn scalar_payoff_rejects_invalid_inputs_without_writing_outputs() {
    let mut c = Context::new();
    unsafe {
        for (kind, option, strike, extra) in [
            (-1, 0, 1.0, 0.0),
            (5, 0, 1.0, 0.0),
            (0, 2, 1.0, 0.0),
            (0, -1, 1.0, 0.0),
            (0, 0, f64::NAN, 0.0),
            (0, 0, f64::INFINITY, 0.0),
            (1, 0, 1.0, f64::NEG_INFINITY),
            (0, 0, 1.0, 1.0),
            (2, 0, 1.0, 1.0),
            (4, 0, 1.0, 1.0),
        ] {
            let mut out = 987;
            assert_eq!(
                itofin_scalar_payoff_new(&mut c, kind, option, strike, extra, &mut out, null_mut()),
                INVALID_ARGUMENT
            );
            assert_eq!(out, 987);
        }
        assert_eq!(
            itofin_scalar_payoff_new(&mut c, 0, 0, 100.0, 0.0, null_mut(), null_mut()),
            INVALID_ARGUMENT
        );
        let id = create(&mut c, 0, 0, -f64::MAX, 0.0);
        for price in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, f64::MAX] {
            let mut out = 876.0;
            assert_eq!(
                itofin_scalar_payoff_value(&mut c, id, price, &mut out, null_mut()),
                INVALID_ARGUMENT
            );
            assert_eq!(out, 876.0);
        }
        assert_eq!(
            itofin_scalar_payoff_value(&mut c, id, 0.0, null_mut(), null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_scalar_payoff_info(&mut c, id, null_mut(), null_mut()),
            INVALID_ARGUMENT
        );
        let percentage = create(&mut c, 4, 1, f64::MAX, 0.0);
        let mut out = 876.0;
        assert_eq!(
            itofin_scalar_payoff_value(&mut c, percentage, 2.0, &mut out, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(out, 876.0);
    }
}

#[test]
fn scalar_payoff_handles_are_typed_scoped_and_released() {
    let mut c = Context::new();
    let mut other = Context::new();
    unsafe {
        let id = create(&mut c, 1, 0, 10.0, 7.0);
        let wrong = c.insert(42_u64).unwrap();
        for handle in [0, wrong] {
            let mut out = 111.0;
            assert_eq!(
                itofin_scalar_payoff_value(&mut c, handle, 12.0, &mut out, null_mut()),
                INVALID_HANDLE
            );
            assert_eq!(out, 111.0);
        }
        let mut out = 111.0;
        assert_eq!(
            itofin_scalar_payoff_value(&mut other, id, 12.0, &mut out, null_mut()),
            INVALID_HANDLE
        );
        assert_eq!(out, 111.0);
        assert_eq!(itofin_handle_release(&mut c, id, null_mut()), 0);
        assert_eq!(
            itofin_scalar_payoff_value(&mut c, id, 12.0, &mut out, null_mut()),
            INVALID_HANDLE
        );
        assert_eq!(out, 111.0);
    }
}

#[test]
fn scalar_payoff_alignment_errors_preserve_outputs_and_error_storage() {
    let mut c = Context::new();
    let mut storage = [0_u64; 140];
    unsafe {
        let mis_error = storage
            .as_mut_ptr()
            .cast::<u8>()
            .add(1)
            .cast::<ItofinError>();
        let mis_handle = storage.as_mut_ptr().cast::<u8>().add(1).cast::<u64>();
        let mis_value = storage.as_mut_ptr().cast::<u8>().add(1).cast::<f64>();
        let mis_info = storage
            .as_mut_ptr()
            .cast::<u8>()
            .add(1)
            .cast::<ItofinScalarPayoffInfo>();
        let id = create(&mut c, 0, 0, 100.0, 0.0);
        let mut handle = 987;
        assert_eq!(
            itofin_scalar_payoff_new(&mut c, 0, 0, 100.0, 0.0, &mut handle, mis_error),
            INVALID_ARGUMENT
        );
        assert_eq!(handle, 987);
        assert_eq!(
            itofin_scalar_payoff_new(&mut c, 0, 0, 100.0, 0.0, mis_handle, null_mut()),
            INVALID_ARGUMENT
        );
        let mut value = 876.0;
        assert_eq!(
            itofin_scalar_payoff_value(&mut c, id, 110.0, &mut value, mis_error),
            INVALID_ARGUMENT
        );
        assert_eq!(value, 876.0);
        assert_eq!(
            itofin_scalar_payoff_value(&mut c, id, 110.0, mis_value, null_mut()),
            INVALID_ARGUMENT
        );
        let mut info = ItofinScalarPayoffInfo {
            kind: -7,
            option_kind: -8,
            strike: -9.0,
            extra: -10.0,
        };
        assert_eq!(
            itofin_scalar_payoff_info(&mut c, id, &mut info, mis_error),
            INVALID_ARGUMENT
        );
        assert_eq!(
            (info.kind, info.option_kind, info.strike, info.extra),
            (-7, -8, -9.0, -10.0)
        );
        assert_eq!(
            itofin_scalar_payoff_info(&mut c, id, mis_info, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(storage, [0_u64; 140]);
        assert_eq!(
            itofin_scalar_payoff_value(&mut c, id, 110.0, &mut value, null_mut()),
            0
        );
        assert_eq!(value, 10.0);
    }
}

#[test]
fn scalar_payoff_info_rejects_wrong_foreign_and_released_handles() {
    let mut c = Context::new();
    let mut other = Context::new();
    unsafe {
        let id = create(&mut c, 0, 0, 100.0, 0.0);
        let wrong = c.insert(42_u64).unwrap();
        let mut info = ItofinScalarPayoffInfo {
            kind: -7,
            option_kind: -8,
            strike: -9.0,
            extra: -10.0,
        };
        for handle in [0, wrong] {
            assert_eq!(
                itofin_scalar_payoff_info(&mut c, handle, &mut info, null_mut()),
                INVALID_HANDLE
            );
            assert_eq!(
                (info.kind, info.option_kind, info.strike, info.extra),
                (-7, -8, -9.0, -10.0)
            );
        }
        assert_eq!(
            itofin_scalar_payoff_info(&mut other, id, &mut info, null_mut()),
            INVALID_HANDLE
        );
        assert_eq!(
            (info.kind, info.option_kind, info.strike, info.extra),
            (-7, -8, -9.0, -10.0)
        );
        assert_eq!(itofin_handle_release(&mut c, id, null_mut()), 0);
        assert_eq!(
            itofin_scalar_payoff_info(&mut c, id, &mut info, null_mut()),
            INVALID_HANDLE
        );
        assert_eq!(
            (info.kind, info.option_kind, info.strike, info.extra),
            (-7, -8, -9.0, -10.0)
        );
    }
}
