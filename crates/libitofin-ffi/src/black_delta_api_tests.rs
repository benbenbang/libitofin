use super::*;
use std::ptr::null_mut;

fn create(c: &mut Context, option: i32, convention: i32, deviation: f64) -> u64 {
    let mut id = 999;
    assert_eq!(
        unsafe {
            itofin_black_delta_new(
                c,
                option,
                convention,
                1.421,
                0.997306,
                0.992266,
                deviation,
                &mut id,
                null_mut(),
            )
        },
        0
    );
    id
}
fn query(c: &mut Context, id: u64, kind: i32, input: f64) -> f64 {
    let mut out = 999.;
    let status = unsafe {
        match kind {
            0 => itofin_black_delta_delta_from_strike(c, id, input, &mut out, null_mut()),
            1 => itofin_black_delta_strike_from_delta(c, id, input, &mut out, null_mut()),
            _ => itofin_black_delta_atm_strike(c, id, input as i32, &mut out, null_mut()),
        }
    };
    assert_eq!(status, 0);
    out
}
fn near(a: f64, b: f64, tolerance: f64) {
    assert!((a - b).abs() <= tolerance, "{a:.17} != {b:.17}");
}
#[test]
fn all_native_quantlib_fixtures_and_roundtrips() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../sdk/go/testdata/black_delta.json")).unwrap();
    let mut c = Context::new();
    let rows = fixture["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 8);
    for row in rows {
        let option = row["option"].as_i64().unwrap() as i32;
        let convention = row["convention"].as_i64().unwrap() as i32;
        let id = create(&mut c, option, convention, 0.1180654);
        near(
            query(&mut c, id, 0, 1.6),
            row["delta"].as_f64().unwrap(),
            1e-14,
        );
        let signed = if option == 0 { 0.15 } else { -0.15 };
        let strike = query(&mut c, id, 1, signed);
        near(strike, row["strike"].as_f64().unwrap(), 1e-10);
        near(query(&mut c, id, 0, strike), signed, 1e-8 * signed.abs());
        for atm in 1..=6 {
            if atm == 6 && convention != 1 {
                continue;
            }
            let expected = match atm {
                1 => 1.421,
                2 => 1.413818813884605,
                3 if convention >= 2 => 1.4039991534456169,
                _ => 1.4237071536606858,
            };
            near(query(&mut c, id, 2, f64::from(atm)), expected, 1e-14);
        }
    }
}
#[test]
fn premium_adjusted_branches_zero_vol_and_put_call_identities() {
    let mut c = Context::new();
    for convention in 0..4 {
        let call = create(&mut c, 0, convention, 0.1180654);
        let put = create(&mut c, 1, convention, 0.1180654);
        let scale = if convention % 2 == 0 { 0.992266 } else { 1. };
        let expected = if convention < 2 {
            scale
        } else {
            scale * 1.6 / 1.413818813884605
        };
        near(
            query(&mut c, call, 0, 1.6) - query(&mut c, put, 0, 1.6),
            expected,
            1e-14,
        );
        if convention >= 2 {
            let low = query(&mut c, call, 0, 0.5);
            let high = query(&mut c, call, 1, low);
            assert!(high > 1.4);
            near(query(&mut c, call, 0, high), low, 1e-10);
        }
        let mut id = 0;
        assert_eq!(
            unsafe {
                itofin_black_delta_new(&mut c, 1, convention, 1., 1., 1., 0., &mut id, null_mut())
            },
            0
        );
        near(query(&mut c, id, 0, 1.), -0.5, 0.);
        near(
            query(&mut c, id, 0, 2.),
            if convention < 2 { -1. } else { -2. },
            0.,
        );
    }
}
#[test]
fn invalid_inputs_preserve_all_output_sentinels() {
    let mut c = Context::new();
    for parameter in 0..4 {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.] {
            let mut args = [1., 1., 1., 0.1];
            args[parameter] = bad;
            let mut id = 999;
            assert_eq!(
                unsafe {
                    itofin_black_delta_new(
                        &mut c,
                        0,
                        0,
                        args[0],
                        args[1],
                        args[2],
                        args[3],
                        &mut id,
                        null_mut(),
                    )
                },
                CORE_ERROR
            );
            assert_eq!(id, 999);
        }
    }
    for (option, convention) in [(2, 0), (-1, 0), (0, 4), (0, i32::MAX)] {
        let mut id = 999;
        assert_eq!(
            unsafe {
                itofin_black_delta_new(
                    &mut c,
                    option,
                    convention,
                    1.,
                    1.,
                    1.,
                    0.1,
                    &mut id,
                    null_mut(),
                )
            },
            INVALID_ARGUMENT
        );
        assert_eq!(id, 999);
    }
    let id = create(&mut c, 0, 0, 0.1180654);
    for bad in [f64::NAN, f64::INFINITY, -1.] {
        let mut out = 999.;
        assert_eq!(
            unsafe { itofin_black_delta_delta_from_strike(&mut c, id, bad, &mut out, null_mut()) },
            CORE_ERROR
        );
        assert_eq!(out, 999.);
    }
    for bad in [f64::NAN, f64::INFINITY, -0.25, 0., 1., 2.] {
        let mut out = 999.;
        assert_eq!(
            unsafe { itofin_black_delta_strike_from_delta(&mut c, id, bad, &mut out, null_mut()) },
            CORE_ERROR
        );
        assert_eq!(out, 999.);
    }
    for atm in [-1, 0, 6, 7, i32::MAX] {
        let mut out = 999.;
        assert_ne!(
            unsafe { itofin_black_delta_atm_strike(&mut c, id, atm, &mut out, null_mut()) },
            0
        );
        assert_eq!(out, 999.);
    }
    assert_eq!(
        unsafe { itofin_black_delta_delta_from_strike(&mut c, id, 1., null_mut(), null_mut()) },
        INVALID_ARGUMENT
    );
    let mut out = 999.;
    let bad = std::ptr::dangling_mut::<u8>().cast::<f64>();
    assert_eq!(
        unsafe { itofin_black_delta_delta_from_strike(&mut c, id, 1., bad, null_mut()) },
        INVALID_ARGUMENT
    );
    let bad_error = std::ptr::dangling_mut::<u8>().cast::<ItofinError>();
    assert_eq!(
        unsafe { itofin_black_delta_delta_from_strike(&mut c, id, 1., &mut out, bad_error) },
        INVALID_ARGUMENT
    );
    assert_eq!(out, 999.);
    near(query(&mut c, id, 0, 1.6), 0.16013974647836293, 1e-14);
}
#[test]
fn wrong_stale_foreign_handles_and_wrong_thread_do_not_write() {
    let mut c = Context::new();
    let id = create(&mut c, 0, 1, 0.1180654);
    let wrong = c.insert(42_u32).unwrap();
    let mut foreign = Context::new();
    for (ctx, handle) in [(&mut c, wrong), (&mut foreign, id)] {
        let mut out = 999.;
        assert_eq!(
            unsafe { itofin_black_delta_delta_from_strike(ctx, handle, 1., &mut out, null_mut()) },
            INVALID_HANDLE
        );
        assert_eq!(out, 999.);
    }
    let address = (&mut c as *mut Context) as usize;
    std::thread::spawn(move || {
        let mut out = 999.;
        assert_eq!(
            unsafe {
                itofin_black_delta_delta_from_strike(
                    address as *mut Context,
                    id,
                    1.,
                    &mut out,
                    null_mut(),
                )
            },
            WRONG_THREAD
        );
        assert_eq!(out, 999.);
    })
    .join()
    .unwrap();
    assert_eq!(
        unsafe { crate::boundary::itofin_handle_release(&mut c, id, null_mut()) },
        0
    );
    let mut out = 999.;
    assert_eq!(
        unsafe { itofin_black_delta_delta_from_strike(&mut c, id, 1., &mut out, null_mut()) },
        INVALID_HANDLE
    );
    assert_eq!(out, 999.);
}

#[test]
fn extreme_finite_inputs_and_zero_discount_domains() {
    let mut c = Context::new();
    for args in [
        [0., 1., 1., 0.1],
        [1., 0., 1., 0.1],
        [1., 1., 0., 0.1],
        [f64::MAX, f64::MIN_POSITIVE, 1., 0.1],
        [1., 1., 1., f64::MAX],
    ] {
        let mut id = 999;
        assert_eq!(
            unsafe {
                itofin_black_delta_new(
                    &mut c,
                    0,
                    1,
                    args[0],
                    args[1],
                    args[2],
                    args[3],
                    &mut id,
                    null_mut(),
                )
            },
            CORE_ERROR
        );
        assert_eq!(id, 999);
    }
    let mut id = 0;
    assert_eq!(
        unsafe { itofin_black_delta_new(&mut c, 0, 1, 1e-300, 1., 1., 0.1, &mut id, null_mut()) },
        0
    );
    let mut out = 999.;
    assert_eq!(
        unsafe { itofin_black_delta_delta_from_strike(&mut c, id, f64::MAX, &mut out, null_mut()) },
        CORE_ERROR
    );
    assert_eq!(out, 999.);
    assert_eq!(
        unsafe { itofin_black_delta_atm_strike(null_mut(), id, 1, &mut out, null_mut()) },
        INVALID_ARGUMENT
    );
    assert_eq!(out, 999.);
}
