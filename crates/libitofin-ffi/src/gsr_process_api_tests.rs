use super::*;
use std::ptr::{null, null_mut};

fn create(c: &mut Context, a: f64, sigma: f64, horizon: f64) -> u64 {
    let mut id = 91;
    assert_eq!(
        unsafe {
            itofin_gsr_process_new(c, null(), 0, &sigma, 1, &a, 1, horizon, &mut id, null_mut())
        },
        0
    );
    id
}
fn query(c: &mut Context, id: u64, kind: i32, t: f64, x: f64, dt: f64, dw: f64) -> f64 {
    let mut out = 91.;
    assert_eq!(
        unsafe { itofin_gsr_process_query(c, id, kind, t, x, dt, dw, &mut out, null_mut()) },
        0
    );
    out
}
fn near(got: f64, want: f64) {
    assert!(
        (got - want).abs() <= 2e-14 * want.abs().max(1e-300),
        "{got} != {want}"
    );
}

#[test]
fn zero_signed_and_regular_reversion_preserve_native_transitions() {
    let mut c = Context::new();
    for a in [0., 1e-15, 0.2, -0.2] {
        let id = create(&mut c, a, 0.03, 10.);
        assert_eq!(
            query(&mut c, id, 0, f64::NAN, f64::NAN, f64::NAN, f64::NAN),
            0.
        );
        assert_eq!(query(&mut c, id, 1, 0., 0., 0., 0.), 10.);
        assert_eq!(query(&mut c, id, 2, 10., 0., 0., 0.), 0.03);
        assert_eq!(query(&mut c, id, 3, 0., 0., 0., 0.), a);
        let kernel = |k: f64, d: f64| if k == 0. { d } else { -(-k * d).exp_m1() / k };
        near(
            query(&mut c, id, 4, 2., 0., 0., 0.),
            0.03_f64.powi(2) * kernel(2. * a, 2.),
        );
        near(query(&mut c, id, 5, 2., 0., 2.8, 0.), kernel(a, 0.8));
        near(
            query(&mut c, id, 6, 2., 0.1, 0., 0.),
            0.03_f64.powi(2) * (kernel(2. * a, 2.) - kernel(a, 8.)) - a * 0.1,
        );
        assert_eq!(query(&mut c, id, 7, 2., 0.1, 0., 0.), 0.03);
        let mean = query(&mut c, id, 8, 2., 0.1, 0.8, 0.);
        let v = 0.03_f64.powi(2) * kernel(2. * a, 0.8);
        near(query(&mut c, id, 9, 2., 0.1, 0.8, 0.), v.sqrt());
        near(query(&mut c, id, 10, 2., 0.1, 0.8, 0.), v);
        near(
            query(&mut c, id, 11, 2., 0.1, 0.8, -0.7),
            mean - v.sqrt() * 0.7,
        );
        assert_eq!(query(&mut c, id, 11, 2., 0.1, 0., 4.), 0.1);
        if a == 0. {
            near(
                mean,
                0.1 + 0.03_f64.powi(2) * (2. * 0.8 - 0.8 * (10. - 2. - 0.8)),
            );
        }
    }
}

#[test]
fn setter_uses_native_horizon_validation_and_updates_dynamics() {
    let mut c = Context::new();
    let id = create(&mut c, 0., 0.03, 10.);
    let before = query(&mut c, id, 8, 0., 0.1, 1., 0.);
    assert_eq!(
        unsafe { itofin_gsr_process_set_forward_measure_time(&mut c, id, 5., null_mut()) },
        0
    );
    assert_ne!(query(&mut c, id, 8, 0., 0.1, 1., 0.), before);
    let fresh = create(&mut c, 0., 0.03, 5.);
    assert_eq!(
        query(&mut c, id, 8, 0., 0.1, 1., 0.),
        query(&mut c, fresh, 8, 0., 0.1, 1., 0.)
    );
    for bad in [-1., f64::NAN, f64::INFINITY] {
        assert_eq!(
            unsafe { itofin_gsr_process_set_forward_measure_time(&mut c, id, bad, null_mut()) },
            CORE_ERROR
        );
        assert_eq!(query(&mut c, id, 1, 0., 0., 0., 0.), 5.);
    }
    assert_eq!(
        unsafe { itofin_gsr_process_set_forward_measure_time(&mut c, id, 0., null_mut()) },
        0
    );
    assert_eq!(query(&mut c, id, 11, 0., 0.3, 0., 2.), 0.3);
}

#[test]
fn constructor_grid_and_pointer_errors_preserve_output() {
    let mut c = Context::new();
    for (times, vols, rev, horizon) in [
        (vec![], vec![], vec![], 10.),
        (vec![1.], vec![0.03], vec![0.1], 10.),
        (vec![1.], vec![0.03, 0.04], vec![0.1], 10.),
        (vec![1.], vec![0.03, 0.03], vec![0.1, 0.2], 10.),
        (vec![1., 1.], vec![0.03; 3], vec![0.1], 10.),
        (vec![-1.], vec![0.03; 2], vec![0.1], 10.),
        (vec![], vec![-0.03], vec![0.1], 10.),
        (vec![], vec![0.03], vec![f64::NAN], 10.),
        (vec![], vec![0.03], vec![0.1], -1.),
    ] {
        let mut out = 91;
        assert_eq!(
            unsafe {
                itofin_gsr_process_new(
                    &mut c,
                    times.as_ptr(),
                    times.len(),
                    vols.as_ptr(),
                    vols.len(),
                    rev.as_ptr(),
                    rev.len(),
                    horizon,
                    &mut out,
                    null_mut(),
                )
            },
            CORE_ERROR
        );
        assert_eq!(out, 91);
    }
    let mut out = 91;
    assert_eq!(
        unsafe {
            itofin_gsr_process_new(
                &mut c,
                null(),
                1,
                &0.03,
                1,
                &0.1,
                1,
                10.,
                &mut out,
                null_mut(),
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(out, 91);
    assert_eq!(
        unsafe {
            itofin_gsr_process_new(
                &mut c,
                null(),
                0,
                &0.03,
                1,
                &0.1,
                1,
                10.,
                null_mut(),
                null_mut(),
            )
        },
        INVALID_ARGUMENT
    );
    let times = [1., 2.];
    let vols = [0.03; 3];
    let rev = [-0.1; 3];
    assert_eq!(
        unsafe {
            itofin_gsr_process_new(
                &mut c,
                times.as_ptr(),
                2,
                vols.as_ptr(),
                3,
                rev.as_ptr(),
                3,
                10.,
                &mut out,
                null_mut(),
            )
        },
        0
    );
    assert_eq!(query(&mut c, out, 3, 0., 0., 0., 0.), -0.1);
}

#[test]
fn query_errors_and_foreign_stale_wrong_handles_preserve_sentinels() {
    let mut c = Context::new();
    let id = create(&mut c, 0.1, 0.03, 2.);
    let cases = [
        (2, 2.1, 0., 0., 0.),
        (3, -1., 0., 0., 0.),
        (4, f64::NAN, 0., 0., 0.),
        (5, 1., 0., 0.5, 0.),
        (6, 0., f64::INFINITY, 0., 0.),
        (7, -1., 0., 0., 0.),
        (8, 1.5, 0., 0.6, 0.),
        (9, 0., f64::NAN, 1., 0.),
        (10, 0., 0., -1., 0.),
        (11, 0., 0., 1., f64::NAN),
        (99, 0., 0., 0., 0.),
    ];
    for (kind, t, x, dt, dw) in cases {
        let mut out = 91.;
        let mut e = ItofinError {
            code: 0,
            message: [0; 1024],
        };
        let code =
            unsafe { itofin_gsr_process_query(&mut c, id, kind, t, x, dt, dw, &mut out, &mut e) };
        assert_ne!(code, 0);
        assert_eq!(code, e.code);
        assert_eq!(out, 91.);
    }
    let wrong = c.insert(3_f64).unwrap();
    for (ctx, handle) in [(&mut Context::new(), id), (&mut c, wrong)] {
        let mut out = 91.;
        assert_eq!(
            unsafe {
                itofin_gsr_process_query(ctx, handle, 0, 0., 0., 0., 0., &mut out, null_mut())
            },
            INVALID_HANDLE
        );
        assert_eq!(out, 91.);
        assert_eq!(
            unsafe { itofin_gsr_process_set_forward_measure_time(ctx, handle, 1., null_mut()) },
            INVALID_HANDLE
        );
    }
    assert_eq!(
        unsafe { itofin_gsr_process_query(&mut c, id, 0, 0., 0., 0., 0., null_mut(), null_mut()) },
        INVALID_ARGUMENT
    );
    let mut out = 91_f64;
    let misaligned = std::ptr::from_mut(&mut out)
        .cast::<u8>()
        .wrapping_add(1)
        .cast::<f64>();
    assert_eq!(
        unsafe { itofin_gsr_process_query(&mut c, id, 0, 0., 0., 0., 0., misaligned, null_mut()) },
        INVALID_ARGUMENT
    );
    assert_eq!(out, 91.);
    let misaligned_error = std::ptr::from_mut(&mut out)
        .cast::<u8>()
        .wrapping_add(1)
        .cast::<ItofinError>();
    assert_eq!(
        unsafe { itofin_gsr_process_set_forward_measure_time(&mut c, id, 1., misaligned_error) },
        INVALID_ARGUMENT
    );
    assert_eq!(query(&mut c, id, 1, 0., 0., 0., 0.), 2.);
    assert_eq!(unsafe { itofin_handle_release(&mut c, id, null_mut()) }, 0);
    assert_eq!(
        unsafe { itofin_gsr_process_query(&mut c, id, 0, 0., 0., 0., 0., &mut out, null_mut()) },
        INVALID_HANDLE
    );
    assert_eq!(out, 91.);
}

#[test]
fn wrong_thread_is_rejected_before_read_or_mutation() {
    let mut c = Context::new();
    let id = create(&mut c, 0., 0.03, 2.);
    let address = std::ptr::from_mut(&mut c) as usize;
    std::thread::spawn(move || {
        let mut out = 91.;
        let c = address as *mut Context;
        assert_eq!(
            unsafe { itofin_gsr_process_query(c, id, 0, 0., 0., 0., 0., &mut out, null_mut()) },
            WRONG_THREAD
        );
        assert_eq!(out, 91.);
        assert_eq!(
            unsafe { itofin_gsr_process_set_forward_measure_time(c, id, 1., null_mut()) },
            WRONG_THREAD
        );
    })
    .join()
    .unwrap();
    assert_eq!(query(&mut c, id, 1, 0., 0., 0., 0.), 2.);
}
