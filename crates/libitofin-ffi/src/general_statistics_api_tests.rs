use super::*;
use crate::boundary::{INVALID_HANDLE, itofin_handle_release};

fn blank_error() -> ItofinError {
    ItofinError {
        code: 0,
        message: [0; 1024],
    }
}

#[test]
fn weighted_queries_and_atomic_batch() {
    let mut context = Context::new();
    let mut error = blank_error();
    let mut id = 0;
    assert_eq!(
        unsafe { itofin_general_statistics_new(&mut context, &mut id, &mut error) },
        0
    );
    let values = [-10.0, -5.0, 1.0];
    let weights = [1.0, 2.0, 17.0];
    assert_eq!(
        unsafe {
            itofin_general_statistics_add_batch(
                &mut context,
                id,
                values.as_ptr(),
                3,
                weights.as_ptr(),
                3,
                &mut error,
            )
        },
        0
    );
    let mut count = 0;
    let mut sum = 0.0;
    assert_eq!(
        unsafe {
            itofin_general_statistics_summary(&mut context, id, &mut count, &mut sum, &mut error)
        },
        0
    );
    assert_eq!((count, sum), (3, 20.0));
    let mut out = 99.0;
    for (selector, argument, expected) in [
        (2, 0.0, -0.15),
        (3, 0.0, 12.49125),
        (16, 0.9, 5.0),
        (17, 0.9, 10.0),
        (8, 0.5, 1.0),
    ] {
        assert_eq!(
            unsafe {
                itofin_general_statistics_query(
                    &mut context,
                    id,
                    selector,
                    argument,
                    &mut out,
                    &mut error,
                )
            },
            0
        );
        assert!((out - expected).abs() < 1e-10, "selector {selector}: {out}");
    }
    let bad = [2.0, Real::NAN];
    assert_ne!(
        unsafe {
            itofin_general_statistics_add_batch(
                &mut context,
                id,
                bad.as_ptr(),
                2,
                std::ptr::null(),
                0,
                &mut error,
            )
        },
        0
    );
    assert_eq!(
        unsafe {
            itofin_general_statistics_summary(&mut context, id, &mut count, &mut sum, &mut error)
        },
        0
    );
    assert_eq!((count, sum), (3, 20.0));
    assert_ne!(
        unsafe { itofin_general_statistics_query(&mut context, id, 8, 0.0, &mut out, &mut error) },
        0
    );
    assert_eq!(out, 1.0);
    assert_eq!(
        unsafe { itofin_general_statistics_add(&mut context, id, -20.0, 1.0, &mut error) },
        0
    );
    assert_eq!(
        unsafe { itofin_general_statistics_query(&mut context, id, 8, 0.01, &mut out, &mut error) },
        0
    );
    assert_eq!(out, -20.0);
    assert_eq!(
        unsafe { itofin_general_statistics_reset(&mut context, id, &mut error) },
        0
    );
    assert_eq!(
        unsafe {
            itofin_general_statistics_summary(&mut context, id, &mut count, &mut sum, &mut error)
        },
        0
    );
    assert_eq!((count, sum), (0, 0.0));
    assert_eq!(
        unsafe { itofin_handle_release(&mut context, id, &mut error) },
        0
    );
    assert_eq!(
        unsafe { itofin_general_statistics_query(&mut context, id, 2, 0.0, &mut out, &mut error) },
        INVALID_HANDLE
    );
}

#[test]
fn rejects_foreign_and_wrong_type_handles() {
    let mut first = Context::new();
    let mut second = Context::new();
    let mut error = blank_error();
    let mut id = 0;
    assert_eq!(
        unsafe { itofin_general_statistics_new(&mut first, &mut id, &mut error) },
        0
    );
    let mut out = 17.0;
    assert_eq!(
        unsafe { itofin_general_statistics_query(&mut second, id, 2, 0.0, &mut out, &mut error) },
        INVALID_HANDLE
    );
    let wrong = first.insert(42_u64).unwrap();
    assert_eq!(
        unsafe { itofin_general_statistics_query(&mut first, wrong, 2, 0.0, &mut out, &mut error) },
        INVALID_HANDLE
    );
    assert_eq!(out, 17.0);
}

#[test]
fn zero_weight_extrema_and_nonfinite_percentiles() {
    let mut context = Context::new();
    let mut error = blank_error();
    let mut id = 0;
    assert_eq!(
        unsafe { itofin_general_statistics_new(&mut context, &mut id, &mut error) },
        0
    );
    assert_eq!(
        unsafe { itofin_general_statistics_add(&mut context, id, -3.0, 0.0, &mut error) },
        0
    );
    assert_eq!(
        unsafe { itofin_general_statistics_add(&mut context, id, 7.0, 0.0, &mut error) },
        0
    );
    let mut out = 91.0;
    for (selector, expected) in [(0, -3.0), (1, 7.0)] {
        assert_eq!(
            unsafe {
                itofin_general_statistics_query(
                    &mut context,
                    id,
                    selector,
                    0.0,
                    &mut out,
                    &mut error,
                )
            },
            0
        );
        assert_eq!(out, expected);
    }
    assert_eq!(
        unsafe { itofin_general_statistics_add(&mut context, id, 2.0, 1.0, &mut error) },
        0
    );
    for selector in [8, 9] {
        for argument in [Real::NAN, Real::INFINITY, Real::NEG_INFINITY] {
            out = 91.0;
            assert_ne!(
                unsafe {
                    itofin_general_statistics_query(
                        &mut context,
                        id,
                        selector,
                        argument,
                        &mut out,
                        &mut error,
                    )
                },
                0
            );
            assert_eq!(out, 91.0);
        }
    }
}

#[test]
fn zero_weight_conditional_tails_report_error_without_output() {
    let mut context = Context::new();
    let mut error = blank_error();
    let mut id = 0;
    assert_eq!(
        unsafe { itofin_general_statistics_new(&mut context, &mut id, &mut error) },
        0
    );
    let values = [-2.0, -1.0, 1.0];
    let weights = [0.0, 0.0, 1.0];
    assert_eq!(
        unsafe {
            itofin_general_statistics_add_batch(
                &mut context,
                id,
                values.as_ptr(),
                3,
                weights.as_ptr(),
                3,
                &mut error,
            )
        },
        0
    );
    for selector in [10, 11, 12, 13, 14, 19] {
        let mut out = 91.0;
        assert_ne!(
            unsafe {
                itofin_general_statistics_query(
                    &mut context,
                    id,
                    selector,
                    0.0,
                    &mut out,
                    &mut error,
                )
            },
            0
        );
        assert_eq!(out, 91.0);
    }
}
