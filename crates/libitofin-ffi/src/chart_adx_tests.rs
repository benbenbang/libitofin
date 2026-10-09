use super::itofin_chart_adx;
use crate::boundary::ItofinError;
use std::ptr::{null, null_mut};

fn error() -> ItofinError {
    ItofinError {
        code: 0,
        message: [0; 1024],
    }
}

#[test]
fn adx_channel_major_fixture_matches_core_bits_and_validity() {
    let h = [10.0, 12.0, 11.0, 14.0, 15.0, 13.0, 18.0, 16.0];
    let l = [8.0, 9.0, 7.0, 10.0, 9.0, 9.0, 15.0, 13.0];
    let c = [9.0, 11.0, 8.0, 13.0, 10.0, 12.0, 17.0, 14.0];
    let mut out = [99.0; 32];
    let mut valid = [77; 4];
    let status = unsafe {
        itofin_chart_adx(
            h.as_ptr(),
            l.as_ptr(),
            c.as_ptr(),
            8,
            2,
            out.as_mut_ptr(),
            32,
            valid.as_mut_ptr(),
            4,
            &mut error(),
        )
    };
    assert_eq!(status, 0);
    assert_eq!(valid, [2, 2, 2, 3]);
    let core = libitofin::math::chart::adx(&h, &l, &c, 2).unwrap();
    for (actual, series) in
        out.chunks_exact(8)
            .zip([core.plus_di, core.minus_di, core.dx, core.adx])
    {
        assert_eq!(
            actual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            series
                .values
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>()
        );
    }
    assert!((out[31] - 22255.0 / 504.0).abs() < 1e-12);
}

#[test]
fn adx_pointer_capacity_size_and_numerical_failures_are_atomic() {
    let good = [1.0, 2.0];
    let invalid = [f64::NAN, 2.0];
    let h = [f64::MAX, -f64::MAX];
    let l = [0.0, -f64::MAX];
    let c = [0.0, -f64::MAX];
    for case in 0..13 {
        let mut out = [99.0; 8];
        let mut valid = [77; 4];
        let (mut hp, mut lp, mut cp) = (good.as_ptr(), good.as_ptr(), good.as_ptr());
        let (mut n, mut p, mut cap, mut vcap) = (2, 2, 8, 4);
        let (mut op, mut vp) = (out.as_mut_ptr(), valid.as_mut_ptr());
        match case {
            0 => hp = null(),
            1 => lp = null(),
            2 => cp = null(),
            3 => op = null_mut(),
            4 => vp = null_mut(),
            5 => cap = 7,
            6 => vcap = 3,
            7 => p = 0,
            8 => {
                hp = invalid.as_ptr();
                p = 14;
            }
            9 => {
                hp = h.as_ptr();
                lp = l.as_ptr();
                cp = c.as_ptr();
                p = 14;
            }
            10 => n = usize::MAX,
            11 => {
                hp = null();
                lp = null();
                cp = null();
                n = 0;
                p = 0;
                op = null_mut();
                cap = 0;
            }
            _ => vcap = 0,
        }
        let status = unsafe { itofin_chart_adx(hp, lp, cp, n, p, op, cap, vp, vcap, &mut error()) };
        assert_ne!(status, 0, "case {case}");
        assert_eq!(out, [99.0; 8], "case {case}");
        assert_eq!(valid, [77; 4], "case {case}");
    }
}

#[test]
fn adx_empty_short_period_one_and_null_error() {
    let mut valid = [77; 4];
    assert_eq!(
        unsafe {
            itofin_chart_adx(
                null(),
                null(),
                null(),
                0,
                14,
                null_mut(),
                0,
                valid.as_mut_ptr(),
                4,
                null_mut(),
            )
        },
        0
    );
    assert_eq!(valid, [0; 4]);
    let c = [1.0, 2.0];
    let mut out = [99.0; 8];
    for p in [1, 14] {
        assert_eq!(
            unsafe {
                itofin_chart_adx(
                    c.as_ptr(),
                    c.as_ptr(),
                    c.as_ptr(),
                    2,
                    p,
                    out.as_mut_ptr(),
                    8,
                    valid.as_mut_ptr(),
                    4,
                    null_mut(),
                )
            },
            0
        );
        assert_eq!(valid, [if p == 1 { 1 } else { 2 }; 4]);
        assert_eq!(
            out,
            if p == 1 {
                [0.0, 100.0, 0.0, 0.0, 0.0, 100.0, 0.0, 100.0]
            } else {
                [0.0; 8]
            }
        );
    }
}

#[test]
fn adx_misaligned_error_pointer_cannot_write_outputs() {
    let c = [1.0, 2.0];
    let mut out = [99.0; 8];
    let mut valid = [77; 4];
    let mut storage = [0usize; 140];
    let error = unsafe {
        storage
            .as_mut_ptr()
            .cast::<u8>()
            .add(1)
            .cast::<ItofinError>()
    };
    assert_ne!(
        unsafe {
            itofin_chart_adx(
                c.as_ptr(),
                c.as_ptr(),
                c.as_ptr(),
                2,
                1,
                out.as_mut_ptr(),
                8,
                valid.as_mut_ptr(),
                4,
                error,
            )
        },
        0
    );
    assert_eq!(out, [99.0; 8]);
    assert_eq!(valid, [77; 4]);
    assert_eq!(storage, [0usize; 140]);
}
