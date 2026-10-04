use super::*;
use crate::boundary::INVALID_ARGUMENT;

fn blank_error() -> ItofinError {
    ItofinError {
        code: 0,
        message: [0; 1024],
    }
}

#[test]
fn batch_checkpoints_empty_and_weights() {
    let values = [2.0, 4.0, 8.0, 10.0];
    let weights = [1.0, 2.0, 1.0, 0.0];
    let mut counts = [99; 2];
    let mut means = [99.0; 2];
    let mut error = blank_error();
    assert_eq!(
        unsafe {
            itofin_convergence_statistics_evaluate(
                values.as_ptr(),
                4,
                weights.as_ptr(),
                4,
                counts.as_mut_ptr(),
                means.as_mut_ptr(),
                2,
                &mut error,
            )
        },
        0
    );
    assert_eq!(counts, [1, 3]);
    assert_eq!(means, [2.0, 4.5]);
    assert_eq!(
        unsafe {
            itofin_convergence_statistics_evaluate(
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
                &mut error,
            )
        },
        0
    );
    assert_eq!(
        unsafe {
            itofin_convergence_statistics_evaluate(
                values.as_ptr(),
                4,
                std::ptr::null(),
                0,
                counts.as_mut_ptr(),
                means.as_mut_ptr(),
                2,
                &mut error,
            )
        },
        0
    );
    assert_eq!(means[0], 2.0);
    assert!((means[1] - 14.0 / 3.0).abs() < 1e-14);
}

#[test]
fn batch_errors_preserve_both_outputs_before_pointer_access() {
    let values = [2.0, 4.0, 8.0];
    let weights = [0.0; 3];
    let mut counts = [91; 2];
    let mut means = [91.0; 2];
    let mut error = blank_error();
    let bogus = std::ptr::dangling::<Real>();
    let cases = [
        (bogus, 100001, std::ptr::null(), 0, 2),
        (values.as_ptr(), 3, std::ptr::null(), 0, 1),
        (std::ptr::null(), 3, std::ptr::null(), 0, 2),
        (values.as_ptr(), 3, std::ptr::null(), 3, 2),
        (values.as_ptr(), 3, weights.as_ptr(), 2, 2),
        (values.as_ptr(), 3, weights.as_ptr(), 3, 2),
    ];
    for (input, len, weight, wlen, out_len) in cases {
        let status = unsafe {
            itofin_convergence_statistics_evaluate(
                input,
                len,
                weight,
                wlen,
                counts.as_mut_ptr(),
                means.as_mut_ptr(),
                out_len,
                &mut error,
            )
        };
        assert_ne!(status, 0);
        assert_eq!(error.code, status);
        assert_eq!(counts, [91; 2]);
        assert_eq!(means, [91.0; 2]);
    }
    let invalid = [2.0, Real::NAN, 8.0];
    assert_ne!(
        unsafe {
            itofin_convergence_statistics_evaluate(
                invalid.as_ptr(),
                3,
                std::ptr::null(),
                0,
                counts.as_mut_ptr(),
                means.as_mut_ptr(),
                2,
                &mut error,
            )
        },
        0
    );
    assert_eq!(counts, [91; 2]);
    assert_eq!(means, [91.0; 2]);
    let misaligned = (std::mem::align_of::<Real>() + 1) as *mut Real;
    assert_eq!(
        unsafe {
            itofin_convergence_statistics_evaluate(
                values.as_ptr(),
                3,
                std::ptr::null(),
                0,
                counts.as_mut_ptr(),
                misaligned,
                2,
                &mut error,
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(counts, [91; 2]);
    assert_eq!(
        unsafe {
            itofin_convergence_statistics_evaluate(
                values.as_ptr(),
                3,
                std::ptr::null(),
                0,
                counts.as_mut_ptr(),
                means.as_mut_ptr(),
                2,
                (std::mem::align_of::<ItofinError>() + 1) as *mut ItofinError,
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(counts, [91; 2]);
    assert_eq!(means, [91.0; 2]);
    assert_eq!(
        unsafe {
            itofin_convergence_statistics_evaluate(
                values.as_ptr(),
                3,
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                means.as_mut_ptr(),
                2,
                &mut error,
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(means, [91.0; 2]);
}
