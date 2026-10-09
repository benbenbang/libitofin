use super::*;

fn sentinel() -> ItofinDrawdownResult {
    ItofinDrawdownResult {
        drawdown: 91.0,
        peak_index: 92,
        trough_index: 93,
    }
}

#[test]
fn drawdown_ffi_parity_snapshot_and_optional_error() {
    for (values, expected) in [
        (
            vec![100.0, 120.0, 90.0, 130.0],
            ItofinDrawdownResult {
                drawdown: 0.25,
                peak_index: 1,
                trough_index: 2,
            },
        ),
        (
            vec![100.0, 80.0, 100.0, 70.0],
            ItofinDrawdownResult {
                drawdown: 0.3,
                peak_index: 0,
                trough_index: 3,
            },
        ),
        (
            vec![100.0, 50.0, 200.0, 100.0],
            ItofinDrawdownResult {
                drawdown: 0.5,
                peak_index: 0,
                trough_index: 1,
            },
        ),
        (
            vec![100.0],
            ItofinDrawdownResult {
                drawdown: 0.0,
                peak_index: 0,
                trough_index: 0,
            },
        ),
    ] {
        let mut result = sentinel();
        for _ in 0..3 {
            assert_eq!(
                unsafe {
                    itofin_maximum_drawdown(
                        values.as_ptr(),
                        values.len(),
                        &mut result,
                        std::ptr::null_mut(),
                    )
                },
                0
            );
            assert_eq!(result, expected);
        }
    }
}

#[test]
fn drawdown_ffi_atomic_input_and_pointer_errors() {
    let valid = [100.0, 50.0];
    let bad = [100.0, 50.0, Real::NAN];
    let mut error = ItofinError {
        code: 0,
        message: [0; 1024],
    };
    let mut result = sentinel();
    for (pointer, len) in [
        (std::ptr::null(), 0),
        (std::ptr::null(), 2),
        (valid.as_ptr(), usize::MAX),
        (valid.as_ptr().cast::<u8>().wrapping_add(1).cast(), 1),
        (bad.as_ptr(), bad.len()),
    ] {
        assert_ne!(
            unsafe { itofin_maximum_drawdown(pointer, len, &mut result, &mut error) },
            0
        );
        assert_eq!(result, sentinel());
        assert_ne!(error.code, 0);
        assert_ne!(error.message[0], 0);
    }
    for value in [0.0, -1.0, Real::NAN, Real::INFINITY, Real::NEG_INFINITY] {
        assert_ne!(
            unsafe { itofin_maximum_drawdown(&value, 1, &mut result, &mut error) },
            0
        );
        assert_eq!(result, sentinel());
    }
    assert_ne!(
        unsafe { itofin_maximum_drawdown(valid.as_ptr(), 2, std::ptr::null_mut(), &mut error) },
        0
    );
    let misaligned = (&mut result as *mut ItofinDrawdownResult)
        .cast::<u8>()
        .wrapping_add(1)
        .cast();
    assert_ne!(
        unsafe { itofin_maximum_drawdown(valid.as_ptr(), 2, misaligned, &mut error) },
        0
    );
    assert_eq!(result, sentinel());
    let misaligned_error = (&mut error as *mut ItofinError)
        .cast::<u8>()
        .wrapping_add(1)
        .cast();
    assert_ne!(
        unsafe { itofin_maximum_drawdown(valid.as_ptr(), 2, &mut result, misaligned_error) },
        0
    );
    assert_eq!(result, sentinel());
    assert_eq!(
        unsafe { itofin_maximum_drawdown(valid.as_ptr(), 2, &mut result, &mut error) },
        0
    );
    assert_eq!(error.code, 0);
    assert_eq!(error.message[0], 0);
    assert_eq!(result.drawdown, 0.5);
}
