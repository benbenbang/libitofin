use super::itofin_chart_keltner_channels;
use crate::boundary::ItofinError;
use std::ptr::{null, null_mut};

fn error() -> ItofinError {
    ItofinError {
        code: 0,
        message: [0; 1024],
    }
}

#[test]
fn channel_major_rational_fixture_and_core_bits_match() {
    let high = [12.0, 16.0, 11.0, 15.0, 14.0, 14.0];
    let low = [10.0, 14.0, 9.0, 13.0, 12.0, 14.0];
    let close = [11.0, 15.0, 10.0, 14.0, 13.0, 14.0];
    let mut out = [99.0; 18];
    let mut first = 77;
    let mut error = error();
    let status = unsafe {
        itofin_chart_keltner_channels(
            high.as_ptr(),
            low.as_ptr(),
            close.as_ptr(),
            6,
            3,
            2,
            1.5,
            out.as_mut_ptr(),
            18,
            &mut first,
            &mut error,
        )
    };
    assert_eq!(status, 0);
    assert_eq!(first, 2);
    let expected = [
        0.0,
        0.0,
        12.0,
        13.0,
        13.0,
        13.5,
        0.0,
        0.0,
        153.0 / 8.0,
        325.0 / 16.0,
        581.0 / 32.0,
        1077.0 / 64.0,
        0.0,
        0.0,
        39.0 / 8.0,
        91.0 / 16.0,
        251.0 / 32.0,
        651.0 / 64.0,
    ];
    for (actual, expected) in out.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 1e-12);
    }
    let core = libitofin::math::chart::keltner_channels(&high, &low, &close, 3, 2, 1.5).unwrap();
    for (actual, series) in out
        .chunks_exact(6)
        .zip([core.center, core.upper, core.lower])
    {
        assert_eq!(
            actual.iter().copied().map(f64::to_bits).collect::<Vec<_>>(),
            series
                .values
                .iter()
                .copied()
                .map(f64::to_bits)
                .collect::<Vec<_>>()
        );
        assert_eq!(series.first_valid, 2);
    }
}

#[test]
fn empty_short_and_zero_multiplier_have_combined_warmup() {
    let mut first = 77;
    let mut error = error();
    assert_eq!(
        unsafe {
            itofin_chart_keltner_channels(
                null(),
                null(),
                null(),
                0,
                20,
                10,
                2.0,
                null_mut(),
                0,
                &mut first,
                &mut error,
            )
        },
        0
    );
    assert_eq!(first, 0);
    let close = [1.0, 2.0];
    let mut out = [99.0; 6];
    assert_eq!(
        unsafe {
            itofin_chart_keltner_channels(
                close.as_ptr(),
                close.as_ptr(),
                close.as_ptr(),
                2,
                1,
                3,
                0.0,
                out.as_mut_ptr(),
                6,
                &mut first,
                &mut error,
            )
        },
        0
    );
    assert_eq!(first, 2);
    assert_eq!(out, [0.0; 6]);
    assert_eq!(
        unsafe {
            itofin_chart_keltner_channels(
                close.as_ptr(),
                close.as_ptr(),
                close.as_ptr(),
                2,
                1,
                1,
                0.0,
                out.as_mut_ptr(),
                6,
                &mut first,
                &mut error,
            )
        },
        0
    );
    assert_eq!(first, 0);
    assert_eq!(out, [1.0, 2.0, 1.0, 2.0, 1.0, 2.0]);
}

#[test]
fn invalid_pointers_capacity_parameters_and_size_are_atomic() {
    let high = [2.0];
    let low = [0.0];
    let close = [1.0];
    for failure in 0..11 {
        let mut out = [99.0; 3];
        let mut first = 77;
        let mut error = error();
        let status = unsafe {
            itofin_chart_keltner_channels(
                if failure == 0 { null() } else { high.as_ptr() },
                if failure == 1 { null() } else { low.as_ptr() },
                if failure == 2 { null() } else { close.as_ptr() },
                if failure == 10 { usize::MAX } else { 1 },
                if failure == 6 { 0 } else { 1 },
                if failure == 7 { 0 } else { 1 },
                if failure == 8 {
                    -1.0
                } else if failure == 9 {
                    f64::NAN
                } else {
                    2.0
                },
                if failure == 3 {
                    null_mut()
                } else {
                    out.as_mut_ptr()
                },
                if failure == 4 {
                    2
                } else if failure == 10 {
                    usize::MAX
                } else {
                    3
                },
                if failure == 5 { null_mut() } else { &mut first },
                &mut error,
            )
        };
        assert_ne!(status, 0);
        assert_eq!(out, [99.0; 3]);
        assert_eq!(first, 77);
    }
}

#[test]
fn hlc_raw_range_offset_and_band_overflow_are_atomic() {
    for (high, low, close, period, multiplier) in [
        (f64::NAN, 0.0, 1.0, 20, 0.0),
        (2.0, 3.0, 1.0, 20, 0.0),
        (f64::MAX, -f64::MAX, 0.0, 20, 0.0),
        (f64::MAX, 0.0, 0.0, 1, 2.0),
        (f64::MAX, 0.0, f64::MAX, 1, 1.0),
        (0.0, -f64::MAX, -f64::MAX, 1, 1.0),
    ] {
        let mut out = [99.0; 3];
        let mut first = 77;
        let mut error = error();
        assert_ne!(
            unsafe {
                itofin_chart_keltner_channels(
                    [high].as_ptr(),
                    [low].as_ptr(),
                    [close].as_ptr(),
                    1,
                    period,
                    period,
                    multiplier,
                    out.as_mut_ptr(),
                    3,
                    &mut first,
                    &mut error,
                )
            },
            0
        );
        assert_eq!(out, [99.0; 3]);
        assert_eq!(first, 77);
    }
}

#[test]
fn late_overflow_preserves_all_channels() {
    let high = [2.0, f64::MAX];
    let low = [0.0, 0.0];
    let close = [1.0, f64::MAX];
    let mut out = [99.0; 6];
    let mut first = 77;
    let mut error = error();
    assert_ne!(
        unsafe {
            itofin_chart_keltner_channels(
                high.as_ptr(),
                low.as_ptr(),
                close.as_ptr(),
                2,
                1,
                1,
                1.0,
                out.as_mut_ptr(),
                6,
                &mut first,
                &mut error,
            )
        },
        0
    );
    assert_eq!(out, [99.0; 6]);
    assert_eq!(first, 77);
}
