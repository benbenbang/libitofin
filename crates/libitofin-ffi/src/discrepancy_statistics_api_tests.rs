use super::*;
use crate::boundary::INVALID_ARGUMENT;

fn blank_error() -> ItofinError {
    ItofinError {
        code: 0,
        message: [0; 1024],
    }
}

#[test]
fn unit_weights_endpoints_and_known_discrepancy() {
    let values = [0.5, 0.5];
    let mut result = 91.0;
    let mut error = blank_error();
    assert_eq!(
        unsafe {
            itofin_discrepancy_statistics_evaluate(
                values.as_ptr(),
                2,
                1,
                2,
                std::ptr::null(),
                0,
                &mut result,
                &mut error,
            )
        },
        0
    );
    assert!((result - (23.0_f64 / 288.0).sqrt()).abs() < 1e-14);
    let expected = result;
    let weights = [1.0];
    assert_eq!(
        unsafe {
            itofin_discrepancy_statistics_evaluate(
                values.as_ptr(),
                2,
                1,
                2,
                weights.as_ptr(),
                1,
                &mut result,
                &mut error,
            )
        },
        0
    );
    assert_eq!(result, expected);
    let endpoints = [0.0, 0.0, 1.0, 1.0];
    assert_eq!(
        unsafe {
            itofin_discrepancy_statistics_evaluate(
                endpoints.as_ptr(),
                4,
                2,
                2,
                std::ptr::null(),
                0,
                &mut result,
                &mut error,
            )
        },
        0
    );
    assert!(result.is_finite());
}

#[test]
fn shape_work_pointer_and_weight_errors_preserve_result() {
    let values = [0.5, 0.5];
    let weights = [2.0];
    let mut result = 91.0;
    let mut error = blank_error();
    let bogus = std::ptr::dangling::<Real>();
    for (pointer, len, rows, dimension, weight, wlen) in [
        (bogus, usize::MAX, usize::MAX, 2, std::ptr::null(), 0),
        (bogus, 2, 4097, 2, std::ptr::null(), 0),
        (bogus, 2, 4096, 256, std::ptr::null(), 0),
        (bogus, 2, 1, 257, std::ptr::null(), 0),
        (bogus, 2, 4096, 6, std::ptr::null(), 0),
        (values.as_ptr(), 2, 1, 1, std::ptr::null(), 0),
        (values.as_ptr(), 1, 1, 2, std::ptr::null(), 0),
        (std::ptr::null(), 2, 1, 2, std::ptr::null(), 0),
        (values.as_ptr(), 2, 1, 2, std::ptr::null(), 1),
        (values.as_ptr(), 2, 1, 2, weights.as_ptr(), 0),
        (values.as_ptr(), 2, 1, 2, weights.as_ptr(), 1),
    ] {
        let status = unsafe {
            itofin_discrepancy_statistics_evaluate(
                pointer,
                len,
                rows,
                dimension,
                weight,
                wlen,
                &mut result,
                &mut error,
            )
        };
        assert_ne!(status, 0);
        assert_eq!(error.code, status);
        assert_eq!(result, 91.0);
    }
    for invalid in [
        [-0.1, 0.5],
        [1.1, 0.5],
        [Real::NAN, 0.5],
        [Real::INFINITY, 0.5],
    ] {
        assert_ne!(
            unsafe {
                itofin_discrepancy_statistics_evaluate(
                    invalid.as_ptr(),
                    2,
                    1,
                    2,
                    std::ptr::null(),
                    0,
                    &mut result,
                    &mut error,
                )
            },
            0
        );
        assert_eq!(result, 91.0);
    }
    let misaligned = (std::mem::align_of::<Real>() + 1) as *const Real;
    assert_eq!(
        unsafe {
            itofin_discrepancy_statistics_evaluate(
                misaligned,
                2,
                1,
                2,
                std::ptr::null(),
                0,
                &mut result,
                &mut error,
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(result, 91.0);
    assert_eq!(
        unsafe {
            itofin_discrepancy_statistics_evaluate(
                values.as_ptr(),
                2,
                1,
                2,
                std::ptr::null(),
                0,
                &mut result,
                (std::mem::align_of::<ItofinError>() + 1) as *mut ItofinError,
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(result, 91.0);
    assert_eq!(
        unsafe {
            itofin_discrepancy_statistics_evaluate(
                values.as_ptr(),
                2,
                1,
                2,
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                &mut error,
            )
        },
        INVALID_ARGUMENT
    );
}
