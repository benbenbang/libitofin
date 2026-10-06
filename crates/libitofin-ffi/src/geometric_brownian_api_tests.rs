use super::*;
use std::ptr::null_mut;

fn process(c: &mut Context, initial: f64, mu: f64, volatility: f64) -> u64 {
    let mut id = 91;
    assert_eq!(
        unsafe { itofin_geometric_brownian_new(c, initial, mu, volatility, &mut id, null_mut()) },
        0
    );
    id
}

fn query(c: &mut Context, id: u64, kind: i32, t: f64, x: f64, dt: f64, draw: f64) -> f64 {
    let mut out = 91.;
    assert_eq!(
        unsafe {
            itofin_geometric_brownian_query(c, id, kind, t, x, dt, draw, &mut out, null_mut())
        },
        0
    );
    out
}

#[test]
fn scalar_euler_values_and_signed_states_are_exact() {
    let mut c = Context::new();
    let id = process(&mut c, 100., 0.08, 0.2);
    for (kind, expected) in [100_f64, 0.08, 0.2, 8., 20., 102., 100., 10., 109.5]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            query(&mut c, id, kind as i32, 0.5, 100., 0.25, 0.75).to_bits(),
            expected.to_bits()
        );
    }
    assert_eq!(query(&mut c, id, 7, 0.5, -100., 0.25, 0.), -10.);
    assert_eq!(query(&mut c, id, 8, 0., 100., 1., -10.), -92.);
    assert_eq!(
        query(&mut c, id, 8, 0., -0., 0., -0.).to_bits(),
        (-0_f64).to_bits()
    );
    let second = process(&mut c, -10., -0.4, 0.);
    assert_eq!(query(&mut c, second, 0, 0., 0., 0., 0.), -10.);
    assert_eq!(query(&mut c, second, 8, 0., -10., 0.5, 50.), -8.);
}

#[test]
fn handles_are_typed_context_scoped_and_releasable() {
    let mut c = Context::new();
    let mut other = Context::new();
    let id = process(&mut c, 100., 0.08, 0.2);
    let wrong = c.insert(100_f64).unwrap();
    for (ctx, handle) in [(&mut other, id), (&mut c, wrong)] {
        let mut out = 91.;
        assert_eq!(
            unsafe {
                itofin_geometric_brownian_query(
                    ctx,
                    handle,
                    0,
                    0.,
                    0.,
                    0.,
                    0.,
                    &mut out,
                    null_mut(),
                )
            },
            INVALID_HANDLE
        );
        assert_eq!(out, 91.);
    }
    assert_eq!(unsafe { itofin_handle_release(&mut c, id, null_mut()) }, 0);
    let mut out = 91.;
    assert_eq!(
        unsafe {
            itofin_geometric_brownian_query(&mut c, id, 0, 0., 0., 0., 0., &mut out, null_mut())
        },
        INVALID_HANDLE
    );
    assert_eq!(out, 91.);
    let next = process(&mut c, 100., 0.08, 0.2);
    assert_ne!(id, next);
    assert_eq!(query(&mut c, next, 0, 0., 0., 0., 0.), 100.);
}

#[test]
fn invalid_parameters_leave_constructor_output_unchanged() {
    let mut c = Context::new();
    for params in [
        [f64::NAN, 0.1, 0.2],
        [f64::INFINITY, 0.1, 0.2],
        [100., f64::NAN, 0.2],
        [100., f64::NEG_INFINITY, 0.2],
        [100., 0.1, -0.2],
        [100., 0.1, f64::INFINITY],
        [100., 0.1, f64::NAN],
    ] {
        let mut out = 91;
        let mut error = ItofinError {
            code: 0,
            message: [0; 1024],
        };
        assert_eq!(
            unsafe {
                itofin_geometric_brownian_new(
                    &mut c, params[0], params[1], params[2], &mut out, &mut error,
                )
            },
            CORE_ERROR
        );
        assert_eq!(out, 91);
        assert_eq!(error.code, CORE_ERROR);
    }
    let zero = process(&mut c, 0., 0., 0.);
    assert_eq!(query(&mut c, zero, 0, 0., 0., 0., 0.), 0.);
}

#[test]
fn invalid_query_domains_preserve_outputs_and_context_recovers() {
    let mut c = Context::new();
    let id = process(&mut c, 100., 0.08, 0.2);
    let mut cases = Vec::new();
    for kind in 3..=8 {
        for bad in [f64::NAN, f64::INFINITY, -1.] {
            cases.push((kind, bad, 100., 0.25, 0.75));
        }
        for bad in [f64::NAN, f64::NEG_INFINITY] {
            cases.push((kind, 0.5, bad, 0.25, 0.75));
        }
    }
    for kind in 5..=8 {
        for bad in [f64::NAN, f64::INFINITY, -1.] {
            cases.push((kind, 0.5, 100., bad, 0.75));
        }
    }
    for bad in [f64::NAN, f64::INFINITY] {
        cases.push((8, 0.5, 100., 0.25, bad));
    }
    for (kind, t, x, dt, draw) in cases {
        let mut out = 91.;
        assert_eq!(
            unsafe {
                itofin_geometric_brownian_query(
                    &mut c,
                    id,
                    kind,
                    t,
                    x,
                    dt,
                    draw,
                    &mut out,
                    null_mut(),
                )
            },
            CORE_ERROR
        );
        assert_eq!(out, 91.);
    }
    let mut out = 91.;
    assert_eq!(
        unsafe {
            itofin_geometric_brownian_query(&mut c, id, 9, 0., 0., 0., 0., &mut out, null_mut())
        },
        INVALID_ARGUMENT
    );
    assert_eq!(out, 91.);
    for kind in 0..=2 {
        assert!(query(&mut c, id, kind, f64::NAN, f64::NAN, f64::NAN, f64::NAN).is_finite());
    }
    assert_eq!(query(&mut c, id, 8, 0., 100., 0.25, 0.75), 109.5);
}

#[test]
fn null_and_misaligned_outputs_and_null_context_are_rejected() {
    let mut c = Context::new();
    let id = process(&mut c, 100., 0.08, 0.2);
    let mut bytes = [0_u64; 4];
    let bad = unsafe { bytes.as_mut_ptr().cast::<u8>().add(1) };
    for out in [null_mut(), bad.cast::<u64>()] {
        assert_eq!(
            unsafe { itofin_geometric_brownian_new(&mut c, 100., 0.1, 0.2, out, null_mut()) },
            INVALID_ARGUMENT
        );
    }
    for out in [null_mut(), bad.cast::<f64>()] {
        assert_eq!(
            unsafe {
                itofin_geometric_brownian_query(&mut c, id, 0, 0., 0., 0., 0., out, null_mut())
            },
            INVALID_ARGUMENT
        );
    }
    let mut out = 91.;
    assert_eq!(
        unsafe {
            itofin_geometric_brownian_query(null_mut(), id, 0, 0., 0., 0., 0., &mut out, null_mut())
        },
        INVALID_ARGUMENT
    );
    assert_eq!(out, 91.);
    assert_eq!(bytes, [0; 4]);
}

#[test]
fn overflow_errors_preserve_outputs_but_zero_step_is_identity() {
    let mut c = Context::new();
    let id = process(&mut c, f64::MAX, 2., 2.);
    for kind in 3..=8 {
        let mut out = 91.;
        assert_eq!(
            unsafe {
                itofin_geometric_brownian_query(
                    &mut c,
                    id,
                    kind,
                    0.,
                    f64::MAX,
                    1.,
                    1.,
                    &mut out,
                    null_mut(),
                )
            },
            CORE_ERROR
        );
        assert_eq!(out, 91.);
    }
    assert_eq!(query(&mut c, id, 5, 0., f64::MAX, 0., 0.), f64::MAX);
    assert_eq!(query(&mut c, id, 8, 0., f64::MAX, 0., 1.), f64::MAX);
    assert_eq!(query(&mut c, id, 6, 0., f64::MAX, 0., 0.), 0.);
}

#[test]
fn misaligned_error_pointer_does_not_mutate_output() {
    let mut c = Context::new();
    let id = process(&mut c, 100., 0.08, 0.2);
    let mut storage = [0_u64; 130];
    let error = unsafe {
        storage
            .as_mut_ptr()
            .cast::<u8>()
            .add(1)
            .cast::<ItofinError>()
    };
    let mut handle = 91;
    assert_eq!(
        unsafe { itofin_geometric_brownian_new(&mut c, 100., 0.08, 0.2, &mut handle, error) },
        INVALID_ARGUMENT
    );
    assert_eq!(handle, 91);
    let mut out = 91.;
    assert_eq!(
        unsafe { itofin_geometric_brownian_query(&mut c, id, 0, 0., 0., 0., 0., &mut out, error) },
        INVALID_ARGUMENT
    );
    assert_eq!(out, 91.);
    assert_eq!(storage, [0; 130]);
}

#[test]
fn context_rejects_foreign_thread_without_mutation() {
    let mut c = Context::new();
    let id = process(&mut c, 100., 0.08, 0.2);
    let address = (&mut c as *mut Context) as usize;
    std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let mut out = 91.;
                assert_eq!(
                    unsafe {
                        itofin_geometric_brownian_query(
                            address as *mut Context,
                            id,
                            0,
                            0.,
                            0.,
                            0.,
                            0.,
                            &mut out,
                            null_mut(),
                        )
                    },
                    WRONG_THREAD
                );
                assert_eq!(out, 91.);
            })
            .join()
            .unwrap();
    });
    assert_eq!(query(&mut c, id, 0, 0., 0., 0., 0.), 100.);
}
