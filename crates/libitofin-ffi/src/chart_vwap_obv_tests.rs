use super::{itofin_chart_obv, itofin_chart_vwap};
use crate::boundary::ItofinError;
use std::ptr::{null, null_mut};

type VolumeFunction = unsafe extern "C" fn(
    *const f64,
    *const f64,
    usize,
    *mut f64,
    usize,
    *mut usize,
    *mut ItofinError,
) -> i32;

fn error() -> ItofinError {
    ItofinError {
        code: 0,
        message: [0; 1024],
    }
}

#[test]
fn shared_hand_fixture_and_empty_alignment() {
    let prices = [10.0, 12.0, 11.0, 11.0, 9.0];
    let volumes = [0.0, 2.0, 1.0, 0.0, 3.0];
    for (function, expected, valid) in [
        (
            itofin_chart_vwap as VolumeFunction,
            [0.0, 12.0, 35.0 / 3.0, 35.0 / 3.0, 31.0 / 3.0],
            1,
        ),
        (
            itofin_chart_obv as VolumeFunction,
            [0.0, 2.0, 1.0, 1.0, -2.0],
            0,
        ),
    ] {
        let mut output = [99.0; 5];
        let mut first_valid = usize::MAX;
        let mut error = error();
        let status = unsafe {
            function(
                prices.as_ptr(),
                volumes.as_ptr(),
                5,
                output.as_mut_ptr(),
                5,
                &mut first_valid,
                &mut error,
            )
        };
        assert_eq!(status, 0);
        assert_eq!(first_valid, valid);
        for (actual, expected) in output.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-12);
        }
        let status = unsafe {
            function(
                null(),
                null(),
                0,
                null_mut(),
                0,
                &mut first_valid,
                &mut error,
            )
        };
        assert_eq!(status, 0);
        assert_eq!(first_valid, 0);
    }
}

#[test]
fn invalid_capacity_pointers_and_values_preserve_outputs() {
    let prices = [1.0, 2.0];
    let volumes = [3.0, 4.0];
    for function in [itofin_chart_vwap as VolumeFunction, itofin_chart_obv] {
        let mut output = [99.0; 2];
        let mut first_valid = 77;
        let mut error = error();
        for (price, volume, out, capacity, first) in [
            (
                prices.as_ptr(),
                volumes.as_ptr(),
                output.as_mut_ptr(),
                1,
                &mut first_valid as *mut usize,
            ),
            (
                null(),
                volumes.as_ptr(),
                output.as_mut_ptr(),
                2,
                &mut first_valid as *mut usize,
            ),
            (
                prices.as_ptr(),
                null(),
                output.as_mut_ptr(),
                2,
                &mut first_valid as *mut usize,
            ),
            (
                prices.as_ptr(),
                volumes.as_ptr(),
                null_mut(),
                2,
                &mut first_valid as *mut usize,
            ),
            (
                prices.as_ptr(),
                volumes.as_ptr(),
                output.as_mut_ptr(),
                2,
                null_mut(),
            ),
        ] {
            let status = unsafe { function(price, volume, 2, out, capacity, first, &mut error) };
            assert_ne!(status, 0);
            assert_eq!(output, [99.0; 2]);
            assert_eq!(first_valid, 77);
        }
        for invalid in [-1.0, f64::NAN, f64::INFINITY] {
            let bad = [3.0, invalid];
            let status = unsafe {
                function(
                    prices.as_ptr(),
                    bad.as_ptr(),
                    2,
                    output.as_mut_ptr(),
                    2,
                    &mut first_valid,
                    &mut error,
                )
            };
            assert_ne!(status, 0);
            assert_eq!(output, [99.0; 2]);
            assert_eq!(first_valid, 77);
        }
    }
}

#[test]
fn overflow_preserves_every_output_and_missing_prefix_is_explicit() {
    let mut output = [99.0; 3];
    let mut first_valid = 77;
    let mut error = error();
    let prices = [1.0, 2.0, 3.0];
    let volume = [0.0, f64::MAX, f64::MAX];
    for function in [itofin_chart_vwap as VolumeFunction, itofin_chart_obv] {
        let status = unsafe {
            function(
                prices.as_ptr(),
                volume.as_ptr(),
                3,
                output.as_mut_ptr(),
                3,
                &mut first_valid,
                &mut error,
            )
        };
        assert_ne!(status, 0);
        assert_eq!(output, [99.0; 3]);
        assert_eq!(first_valid, 77);
    }
    let status = unsafe {
        itofin_chart_vwap(
            prices.as_ptr(),
            [0.0; 3].as_ptr(),
            3,
            output.as_mut_ptr(),
            3,
            &mut first_valid,
            &mut error,
        )
    };
    assert_eq!(status, 0);
    assert_eq!(output, [0.0; 3]);
    assert_eq!(first_valid, 3);
}
