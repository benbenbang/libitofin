use super::{itofin_chart_atr, itofin_chart_true_range};
use crate::boundary::ItofinError;
use std::ptr::{null, null_mut};

fn error() -> ItofinError {
    ItofinError {
        code: 0,
        message: [0; 1024],
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn call(
    high: *const f64,
    low: *const f64,
    close: *const f64,
    len: usize,
    period: Option<usize>,
    out: *mut f64,
    capacity: usize,
    first: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        if let Some(period) = period {
            itofin_chart_atr(high, low, close, len, period, out, capacity, first, error)
        } else {
            itofin_chart_true_range(high, low, close, len, out, capacity, first, error)
        }
    }
}

#[test]
fn independent_hand_fixture_matches_both_scalar_outputs() {
    let high = [12.0, 16.0, 11.0, 15.0, 14.0, 14.0];
    let low = [10.0, 14.0, 9.0, 13.0, 12.0, 14.0];
    let close = [11.0, 15.0, 10.0, 14.0, 13.0, 14.0];
    for (period, expected, valid) in [
        (None, [2.0, 5.0, 6.0, 5.0, 2.0, 1.0], 0),
        (
            Some(3),
            [0.0, 0.0, 13.0 / 3.0, 41.0 / 9.0, 100.0 / 27.0, 227.0 / 81.0],
            2,
        ),
    ] {
        let mut out = [99.0; 6];
        let mut first = 77;
        let mut error = error();
        let status = unsafe {
            call(
                high.as_ptr(),
                low.as_ptr(),
                close.as_ptr(),
                6,
                period,
                out.as_mut_ptr(),
                6,
                &mut first,
                &mut error,
            )
        };
        assert_eq!(status, 0);
        assert_eq!(first, valid);
        let core = if let Some(period) = period {
            libitofin::math::chart::atr(&high, &low, &close, period).unwrap()
        } else {
            libitofin::math::chart::true_range(&high, &low, &close).unwrap()
        };
        assert_eq!(
            out.map(f64::to_bits).as_slice(),
            core.values
                .iter()
                .copied()
                .map(f64::to_bits)
                .collect::<Vec<_>>()
        );
        for (actual, expected) in out.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-12);
        }
    }
}

#[test]
fn empty_and_short_outputs_follow_alignment_contract() {
    for period in [None, Some(14)] {
        let mut first = 77;
        let mut error = error();
        assert_eq!(
            unsafe {
                call(
                    null(),
                    null(),
                    null(),
                    0,
                    period,
                    null_mut(),
                    0,
                    &mut first,
                    &mut error,
                )
            },
            0
        );
        assert_eq!(first, 0);
    }
    let mut out = [99.0; 2];
    let mut first = 77;
    let mut error = error();
    assert_eq!(
        unsafe {
            itofin_chart_atr(
                [2.0, 3.0].as_ptr(),
                [0.0, 1.0].as_ptr(),
                [1.0, 2.0].as_ptr(),
                2,
                3,
                out.as_mut_ptr(),
                2,
                &mut first,
                &mut error,
            )
        },
        0
    );
    assert_eq!(out, [0.0; 2]);
    assert_eq!(first, 2);
}

#[test]
fn invalid_capacity_and_each_pointer_preserve_outputs() {
    let high = [2.0, 3.0];
    let low = [0.0, 1.0];
    let close = [1.0, 2.0];
    for period in [None, Some(3)] {
        for failure in 0..6 {
            let mut out = [99.0; 2];
            let mut first = 77;
            let mut error = error();
            let status = unsafe {
                call(
                    if failure == 0 { null() } else { high.as_ptr() },
                    if failure == 1 { null() } else { low.as_ptr() },
                    if failure == 2 { null() } else { close.as_ptr() },
                    2,
                    period,
                    if failure == 3 {
                        null_mut()
                    } else {
                        out.as_mut_ptr()
                    },
                    if failure == 4 { 1 } else { 2 },
                    if failure == 5 { null_mut() } else { &mut first },
                    &mut error,
                )
            };
            assert_ne!(status, 0);
            assert_eq!(out, [99.0; 2]);
            assert_eq!(first, 77);
        }
    }
}

#[test]
fn validation_errors_and_extreme_length_preserve_outputs() {
    for period in [None, Some(14)] {
        for (high, low, close) in [
            ([2.0], [3.0], [1.0]),
            ([f64::NAN], [0.0], [1.0]),
            ([2.0], [f64::NEG_INFINITY], [1.0]),
            ([2.0], [0.0], [f64::INFINITY]),
            ([f64::MAX], [-f64::MAX], [0.0]),
        ] {
            let mut out = [99.0];
            let mut first = 77;
            let mut error = error();
            assert_ne!(
                unsafe {
                    call(
                        high.as_ptr(),
                        low.as_ptr(),
                        close.as_ptr(),
                        1,
                        period,
                        out.as_mut_ptr(),
                        1,
                        &mut first,
                        &mut error,
                    )
                },
                0
            );
            assert_eq!(out, [99.0]);
            assert_eq!(first, 77);
        }
        let mut out = [99.0];
        let mut first = 77;
        let mut error = error();
        assert_ne!(
            unsafe {
                call(
                    [2.0].as_ptr(),
                    [0.0].as_ptr(),
                    [1.0].as_ptr(),
                    usize::MAX,
                    period,
                    out.as_mut_ptr(),
                    usize::MAX,
                    &mut first,
                    &mut error,
                )
            },
            0
        );
        assert_eq!(out, [99.0]);
        assert_eq!(first, 77);
    }
    let mut first = 77;
    let mut error = error();
    assert_ne!(
        unsafe {
            itofin_chart_atr(
                null(),
                null(),
                null(),
                0,
                0,
                null_mut(),
                0,
                &mut first,
                &mut error,
            )
        },
        0
    );
    assert_eq!(first, 77);
}

#[test]
fn subnormal_period_one_and_maximum_seed_stay_representable() {
    let high = [f64::MAX, f64::from_bits(1)];
    let zero = [0.0; 2];
    let mut out = [99.0; 2];
    let mut first = 77;
    let mut error = error();
    assert_eq!(
        unsafe {
            itofin_chart_atr(
                high.as_ptr(),
                zero.as_ptr(),
                zero.as_ptr(),
                2,
                1,
                out.as_mut_ptr(),
                2,
                &mut first,
                &mut error,
            )
        },
        0
    );
    assert_eq!(out[0], f64::MAX);
    assert_eq!(out[1].to_bits(), 1);
    let mut out = [99.0; 3];
    assert_eq!(
        unsafe {
            itofin_chart_atr(
                [f64::MAX; 3].as_ptr(),
                [0.0; 3].as_ptr(),
                [0.0; 3].as_ptr(),
                3,
                3,
                out.as_mut_ptr(),
                3,
                &mut first,
                &mut error,
            )
        },
        0
    );
    assert_eq!(out, [0.0, 0.0, f64::MAX]);
    assert_eq!(first, 2);
}
