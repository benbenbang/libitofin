use super::*;
use crate::boundary::{CORE_ERROR, INVALID_ARGUMENT};

fn blank_error() -> ItofinError {
    ItofinError {
        code: 0,
        message: [0; 1024],
    }
}

#[test]
fn weighted_vectors_and_row_major_matrices_match_hand_calculation() {
    let values = [1.0, 2.0, 3.0, 6.0, 5.0, 10.0];
    let weights = [1.0, 2.0, 1.0];
    let variance = 3.0_f64;
    let vectors = [
        [3.0, 6.0],
        [variance, 4.0 * variance],
        [variance.sqrt(), 2.0 * variance.sqrt()],
        [1.0, 2.0],
        [1.0, 2.0],
        [5.0, 10.0],
    ];
    let mut error = blank_error();
    for (measure, expected) in vectors.iter().enumerate() {
        let mut out = [91.0; 2];
        let status = unsafe {
            itofin_sequence_statistics_evaluate(
                values.as_ptr(),
                6,
                3,
                2,
                weights.as_ptr(),
                3,
                measure as i32,
                out.as_mut_ptr(),
                2,
                &mut error,
            )
        };
        assert_eq!(status, 0);
        assert_eq!(error.code, 0);
        for (got, want) in out.iter().zip(expected) {
            assert!((got - want).abs() < 1e-14);
        }
    }
    for (measure, expected) in [(6, [3.0, 6.0, 6.0, 12.0]), (7, [1.0; 4])] {
        let mut out = [91.0; 4];
        let status = unsafe {
            itofin_sequence_statistics_evaluate(
                values.as_ptr(),
                6,
                3,
                2,
                weights.as_ptr(),
                3,
                measure,
                out.as_mut_ptr(),
                4,
                &mut error,
            )
        };
        assert_eq!(status, 0);
        for (got, want) in out.iter().zip(expected) {
            assert!((got - want).abs() < 1e-14);
        }
    }
    let mut out = [91.0; 2];
    assert_eq!(
        unsafe {
            itofin_sequence_statistics_evaluate(
                values.as_ptr(),
                6,
                3,
                2,
                std::ptr::null(),
                0,
                0,
                out.as_mut_ptr(),
                2,
                std::ptr::null_mut(),
            )
        },
        0
    );
    for (got, want) in out.iter().zip([3.0, 6.0]) {
        assert!((got - want).abs() < 1e-14);
    }
}

#[test]
fn invalid_shapes_and_buffers_are_rejected_before_reading_inputs() {
    let dangling = std::ptr::dangling::<Real>();
    let mut out = [91.0; 4];
    let mut error = blank_error();
    for (values_len, rows, dimension, measure, out_len) in [
        (0, 0, 2, 0, 2),
        (2, 1, 0, 0, 2),
        (257, 1, 257, 0, 257),
        (100_001, 100_001, 1, 0, 1),
        (1_000_010, 100_000, 11, 0, 11),
        (781_312, 6104, 128, 6, 16384),
        (781_312, 6104, 128, 7, 16384),
        (usize::MAX, usize::MAX, usize::MAX, 0, 1),
        (4, 2, 2, 8, 2),
        (3, 2, 2, 0, 2),
        (4, 2, 2, 0, 1),
        (4, 2, 2, 0, 3),
        (4, 2, 2, 6, 3),
        (4, 2, 2, 6, 5),
    ] {
        let status = unsafe {
            itofin_sequence_statistics_evaluate(
                dangling,
                values_len,
                rows,
                dimension,
                std::ptr::null(),
                0,
                measure,
                out.as_mut_ptr(),
                out_len,
                &mut error,
            )
        };
        assert!(status == INVALID_ARGUMENT || status == CORE_ERROR);
        assert_eq!(error.code, status);
        assert_eq!(out, [91.0; 4]);
    }
    for (pointer, length) in [(dangling, 0), (dangling, 1), (std::ptr::null(), 2)] {
        assert_eq!(
            unsafe {
                itofin_sequence_statistics_evaluate(
                    dangling,
                    4,
                    2,
                    2,
                    pointer,
                    length,
                    0,
                    out.as_mut_ptr(),
                    2,
                    &mut error,
                )
            },
            INVALID_ARGUMENT
        );
        assert_eq!(out, [91.0; 4]);
    }
}

#[test]
fn invalid_numeric_samples_preserve_all_output_values() {
    let mut error = blank_error();
    for (values, weights, measure) in [
        ([1.0, 2.0, Real::NAN, 4.0], [1.0, 1.0], 0),
        ([1.0, 2.0, Real::INFINITY, 4.0], [1.0, 0.0], 0),
        ([1.0, 2.0, 3.0, 4.0], [-1.0, 2.0], 6),
        ([1.0, 2.0, 3.0, 4.0], [Real::NAN, 1.0], 6),
        ([1.0, 2.0, 3.0, 4.0], [0.0, 0.0], 6),
        ([1.0, 2.0, 3.0, 4.0], [Real::MAX, Real::MAX], 6),
        ([Real::MAX, 2.0, -Real::MAX, 4.0], [1.0, 1.0], 6),
    ] {
        let mut out = [91.0; 4];
        let len = if measure == 0 { 2 } else { 4 };
        let status = unsafe {
            itofin_sequence_statistics_evaluate(
                values.as_ptr(),
                4,
                2,
                2,
                weights.as_ptr(),
                2,
                measure,
                out.as_mut_ptr(),
                len,
                &mut error,
            )
        };
        assert_eq!(status, CORE_ERROR);
        assert_eq!(error.code, status);
        assert_eq!(out, [91.0; 4]);
    }
    for measure in [1, 2, 3, 6, 7] {
        let values = [1.0, 2.0];
        let mut out = [91.0; 4];
        let len = if measure >= 6 { 4 } else { 2 };
        assert_eq!(
            unsafe {
                itofin_sequence_statistics_evaluate(
                    values.as_ptr(),
                    2,
                    1,
                    2,
                    std::ptr::null(),
                    0,
                    measure,
                    out.as_mut_ptr(),
                    len,
                    &mut error,
                )
            },
            CORE_ERROR
        );
        assert_eq!(out, [91.0; 4]);
    }
}

#[test]
fn null_and_misaligned_pointers_preserve_output() {
    let values = [1.0, 2.0, 3.0, 4.0];
    let weights = [1.0, 1.0];
    let mut out = [91.0; 2];
    let mut error = blank_error();
    let misaligned = unsafe { values.as_ptr().cast::<u8>().add(1).cast::<Real>() };
    for pointer in [std::ptr::null(), misaligned] {
        assert_eq!(
            unsafe {
                itofin_sequence_statistics_evaluate(
                    pointer,
                    4,
                    2,
                    2,
                    std::ptr::null(),
                    0,
                    0,
                    out.as_mut_ptr(),
                    2,
                    &mut error,
                )
            },
            INVALID_ARGUMENT
        );
        assert_eq!(out, [91.0; 2]);
    }
    let misaligned_weights = unsafe { weights.as_ptr().cast::<u8>().add(1).cast::<Real>() };
    assert_eq!(
        unsafe {
            itofin_sequence_statistics_evaluate(
                values.as_ptr(),
                4,
                2,
                2,
                misaligned_weights,
                2,
                0,
                out.as_mut_ptr(),
                2,
                &mut error,
            )
        },
        INVALID_ARGUMENT
    );
    let misaligned_out = unsafe { out.as_mut_ptr().cast::<u8>().add(1).cast::<Real>() };
    for pointer in [std::ptr::null_mut(), misaligned_out] {
        assert_eq!(
            unsafe {
                itofin_sequence_statistics_evaluate(
                    values.as_ptr(),
                    4,
                    2,
                    2,
                    std::ptr::null(),
                    0,
                    0,
                    pointer,
                    2,
                    &mut error,
                )
            },
            INVALID_ARGUMENT
        );
        assert_eq!(out, [91.0; 2]);
    }
    let misaligned_error = unsafe {
        (&mut error as *mut ItofinError)
            .cast::<u8>()
            .add(1)
            .cast::<ItofinError>()
    };
    assert_eq!(
        unsafe {
            itofin_sequence_statistics_evaluate(
                values.as_ptr(),
                4,
                2,
                2,
                std::ptr::null(),
                0,
                0,
                out.as_mut_ptr(),
                2,
                misaligned_error,
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(out, [91.0; 2]);
}
