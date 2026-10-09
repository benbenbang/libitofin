use super::itofin_chart_williams_r;
use crate::boundary::ItofinError;

#[test]
fn williams_ffi_fixture_and_atomic_failures() {
    let high = [12.0, 16.0, 11.0];
    let low = [10.0, 14.0, 9.0];
    let close = [11.0, 15.0, 10.0];
    let mut output = [99.0; 3];
    let mut first = 77;
    let mut error = ItofinError {
        code: 0,
        message: [0; 1024],
    };
    let call = |high, low, close, len, period, out, capacity, first, error| unsafe {
        itofin_chart_williams_r(high, low, close, len, period, out, capacity, first, error)
    };
    assert_eq!(
        call(
            high.as_ptr(),
            low.as_ptr(),
            close.as_ptr(),
            3,
            3,
            output.as_mut_ptr(),
            3,
            &mut first,
            &mut error
        ),
        0
    );
    assert_eq!(first, 2);
    assert!((output[2] + 600.0 / 7.0).abs() < 1e-12);
    for (h, l, c, period, capacity) in [
        (high.as_ptr(), low.as_ptr(), close.as_ptr(), 0, 3),
        (high.as_ptr(), low.as_ptr(), close.as_ptr(), 3, 2),
        (std::ptr::null(), low.as_ptr(), close.as_ptr(), 3, 3),
        (high.as_ptr(), std::ptr::null(), close.as_ptr(), 3, 3),
        (high.as_ptr(), low.as_ptr(), std::ptr::null(), 3, 3),
    ] {
        output.fill(99.0);
        first = 77;
        assert_ne!(
            call(
                h,
                l,
                c,
                3,
                period,
                output.as_mut_ptr(),
                capacity,
                &mut first,
                &mut error
            ),
            0
        );
        assert_eq!(output, [99.0; 3]);
        assert_eq!(first, 77);
    }
    assert_eq!(
        call(
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            14,
            std::ptr::null_mut(),
            0,
            &mut first,
            &mut error
        ),
        0
    );
    assert_eq!(first, 0);
}

#[test]
fn williams_ffi_misaligned_error_does_not_publish_output() {
    let high = [2.0];
    let low = [0.0];
    let close = [1.0];
    let mut output = [99.0];
    let mut first = 77;
    let mut error = ItofinError {
        code: 0,
        message: [0; 1024],
    };
    let bad_error = (&mut error as *mut ItofinError)
        .cast::<u8>()
        .wrapping_add(1)
        .cast();
    let code = unsafe {
        itofin_chart_williams_r(
            high.as_ptr(),
            low.as_ptr(),
            close.as_ptr(),
            1,
            1,
            output.as_mut_ptr(),
            1,
            &mut first,
            bad_error,
        )
    };
    assert_ne!(code, 0);
    assert_eq!(output, [99.0]);
    assert_eq!(first, 77);
}
