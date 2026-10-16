use super::*;
use std::ptr::null_mut;

fn v(x: f64, y: f64) -> ItofinG2Vector {
    ItofinG2Vector { values: [x, y] }
}
fn create(c: &mut Context, params: [f64; 5]) -> u64 {
    let mut id = 91;
    assert_eq!(
        unsafe {
            itofin_g2_process_new(
                c,
                params[0],
                params[1],
                params[2],
                params[3],
                params[4],
                &mut id,
                null_mut(),
            )
        },
        0
    );
    id
}
fn vec_query(
    c: &mut Context,
    id: u64,
    kind: i32,
    t: f64,
    x: ItofinG2Vector,
    dt: f64,
    dw: ItofinG2Vector,
) -> [f64; 2] {
    let mut out = v(91., 92.);
    assert_eq!(
        unsafe { itofin_g2_process_vector(c, id, kind, t, x, dt, dw, &mut out, null_mut()) },
        0
    );
    out.values
}
fn mat_query(c: &mut Context, id: u64, kind: i32, t: f64, x: ItofinG2Vector, dt: f64) -> [f64; 4] {
    let mut out = ItofinG2Matrix { values: [91.; 4] };
    assert_eq!(
        unsafe { itofin_g2_process_matrix(c, id, kind, t, x, dt, &mut out, null_mut()) },
        0
    );
    out.values
}
fn near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 8e-14 * expected.abs().max(1e-300),
        "{actual:e} != {expected:e}"
    );
}

#[test]
fn public_coefficients_and_fixed_shape_native_transitions() {
    let mut c = Context::new();
    let id = create(&mut c, [0.1, 0.03, 0.4, 0.007, -0.6]);
    for (kind, want) in [2., 2., 0., 0., 0.1, 0.03, 0.4, 0.007, -0.6]
        .into_iter()
        .enumerate()
    {
        let mut out = 91.;
        assert_eq!(
            unsafe { itofin_g2_process_scalar(&mut c, id, kind as i32, &mut out, null_mut()) },
            0
        );
        assert_eq!(out, want);
    }
    assert_eq!(
        vec_query(
            &mut c,
            id,
            0,
            f64::NAN,
            v(f64::NAN, 0.),
            f64::NAN,
            v(f64::NAN, 0.)
        ),
        [0., 0.]
    );
    assert_eq!(
        vec_query(&mut c, id, 1, 1., v(0.2, -0.3), 0., v(0., 0.)),
        [-0.1 * 0.2, -0.4 * -0.3]
    );
    assert_eq!(
        vec_query(&mut c, id, 3, f64::NAN, v(1., -2.), f64::NAN, v(3., 4.)),
        [4., 2.]
    );
    let d = mat_query(&mut c, id, 0, 0., v(0., 0.), f64::NAN);
    near(d[2], -0.6 * 0.007);
    near(d[2] * d[2] + d[3] * d[3], 0.007_f64.powi(2));
    near(d[0] * d[2], -0.6 * 0.03 * 0.007);
    assert_eq!(std::mem::size_of::<ItofinG2Vector>(), 16);
    assert_eq!(std::mem::size_of::<ItofinG2Matrix>(), 32);
}

#[test]
fn regular_quantlib_exact_steps_and_independent_covariance_integrals() {
    let mut c = Context::new();
    for line in include_str!("../../libitofin/tests/data/g2_native.csv")
        .lines()
        .skip(1)
    {
        let r: Vec<f64> = line.split(',').map(|x| x.parse().unwrap()).collect();
        let id = create(&mut c, [r[0], r[1], r[2], r[3], r[4]]);
        let x = v(r[7], r[8]);
        let e = vec_query(&mut c, id, 2, r[5], x, r[6], v(0., 0.));
        for i in 0..2 {
            near(e[i], r[9 + i]);
        }
        for (kind, offset) in [(1, 15), (2, 11)] {
            let m = mat_query(&mut c, id, kind, r[5], x, r[6]);
            for i in 0..4 {
                near(m[i], r[offset + i]);
            }
        }
    }
    for (a, b, rho) in [
        (0., 0., 1.),
        (0.3, 0.7, -1.),
        (-0.2, 0.2, 0.4),
        (0.3, 0.3, 1.),
    ] {
        let id = create(&mut c, [a, 0.03, b, 0.007, rho]);
        let dt = 0.8;
        let integral = |k: f64| if k == 0. { dt } else { -(-k * dt).exp_m1() / k };
        let expected = [
            0.03_f64.powi(2) * integral(2. * a),
            rho * 0.03 * 0.007 * integral(a + b),
            rho * 0.03 * 0.007 * integral(a + b),
            0.007_f64.powi(2) * integral(2. * b),
        ];
        let cov = mat_query(&mut c, id, 2, 2., v(0., 0.), dt);
        let s = mat_query(&mut c, id, 1, 2., v(0., 0.), dt);
        let gram = [
            s[0] * s[0],
            s[0] * s[2],
            s[0] * s[2],
            s[2] * s[2] + s[3] * s[3],
        ];
        for i in 0..4 {
            near(cov[i], expected[i]);
            near(gram[i], expected[i]);
        }
        let e = vec_query(&mut c, id, 4, 2., v(0., 0.), dt, v(0.75, -0.4));
        near(e[0], s[0] * 0.75);
        near(e[1], s[2] * 0.75 - s[3] * 0.4);
    }
}

#[test]
fn singular_limits_small_residual_and_extreme_scaling_survive_binding() {
    let mut c = Context::new();
    for (sigma, eta) in [(0., 0.), (0., 0.007), (0.03, 0.)] {
        let id = create(&mut c, [0., sigma, 0., eta, -1.]);
        assert_eq!(
            vec_query(&mut c, id, 4, 0., v(0.1, -0.2), 0., v(2., -3.)),
            [0.1, -0.2]
        );
        let m = mat_query(&mut c, id, 2, 0., v(0., 0.), 0.7);
        near(m[0], sigma * sigma * 0.7);
        near(m[3], eta * eta * 0.7);
    }
    let id = create(&mut c, [0.1, 0.03, 0.2, 0.007, 1.]);
    let s = mat_query(&mut c, id, 1, 0., v(0., 0.), 1e-8);
    assert!(s[3] > 0.);
    near(s[3], 2.3094010744491024e-16 * (0.007 / 0.008));
    let id = create(&mut c, [1e200, 1e200, 1e200, 1e200, 0.5]);
    let m = mat_query(&mut c, id, 2, 0., v(0., 0.), 1.);
    near(m[0], 5e199);
    near(m[1], 2.5e199);
    let id = create(&mut c, [-1., 1e-200, -1., 1e-200, 0.5]);
    let e = vec_query(&mut c, id, 2, 0., v(1e-200, -1e-200), 800., v(0., 0.));
    near(e[0], (800. + 1e-200_f64.ln()).exp());
    near(e[1], -e[0]);
}

#[test]
fn constructors_and_all_errors_preserve_complete_sentinel_outputs() {
    let mut c = Context::new();
    for i in 0..5 {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut p = [0.1, 0.03, 0.2, 0.007, 0.5];
            p[i] = bad;
            let mut out = 91;
            assert_eq!(
                unsafe {
                    itofin_g2_process_new(
                        &mut c,
                        p[0],
                        p[1],
                        p[2],
                        p[3],
                        p[4],
                        &mut out,
                        null_mut(),
                    )
                },
                CORE_ERROR
            );
            assert_eq!(out, 91);
        }
    }
    for p in [
        [0., -1., 0., 0., 0.],
        [0., 0., 0., -1., 0.],
        [0., 0., 0., 0., 1.01],
        [0., 0., 0., 0., -1.01],
    ] {
        let mut out = 91;
        assert_eq!(
            unsafe {
                itofin_g2_process_new(&mut c, p[0], p[1], p[2], p[3], p[4], &mut out, null_mut())
            },
            CORE_ERROR
        );
        assert_eq!(out, 91);
    }
    let id = create(&mut c, [0.1, 0.03, 0.2, 0.007, 0.5]);
    let mut cases = vec![
        (99, 0., v(0., 0.), 1., v(0., 0.)),
        (3, 0., v(f64::MAX, 0.), 0., v(f64::MAX, 0.)),
    ];
    for kind in [1, 2, 4] {
        for bad in [-1., f64::NAN, f64::INFINITY] {
            cases.push((kind, bad, v(0., 0.), 1., v(0., 0.)));
        }
        for bad in [f64::NAN, f64::INFINITY] {
            cases.push((kind, 0., v(bad, 0.), 1., v(0., 0.)));
        }
    }
    for kind in [2, 4] {
        for bad in [-1., f64::NAN, f64::INFINITY] {
            cases.push((kind, 0., v(0., 0.), bad, v(0., 0.)));
        }
        cases.push((kind, f64::MAX, v(0., 0.), f64::MAX, v(0., 0.)));
    }
    for bad in [f64::NAN, f64::INFINITY] {
        cases.push((4, 0., v(0., 0.), 1., v(0., bad)));
        cases.push((3, 0., v(0., 0.), 0., v(bad, 0.)));
    }
    for (kind, t, x, dt, dw) in cases {
        let mut out = v(91., 92.);
        let mut error = ItofinError {
            code: 0,
            message: [0; 1024],
        };
        let code = unsafe {
            itofin_g2_process_vector(&mut c, id, kind, t, x, dt, dw, &mut out, &mut error)
        };
        assert_ne!(code, 0);
        assert_eq!(code, error.code);
        assert_eq!(out, v(91., 92.));
    }
    for kind in [0, 1, 2, 99] {
        for (t, x, dt) in [
            (-1., v(0., 0.), 1.),
            (0., v(0., f64::NAN), 1.),
            (0., v(0., 0.), -1.),
        ] {
            if kind == 0 && dt < 0. {
                continue;
            }
            let mut out = ItofinG2Matrix {
                values: [91., 92., 93., 94.],
            };
            assert_ne!(
                unsafe {
                    itofin_g2_process_matrix(&mut c, id, kind, t, x, dt, &mut out, null_mut())
                },
                0
            );
            assert_eq!(out.values, [91., 92., 93., 94.]);
        }
    }
    assert_eq!(
        vec_query(&mut c, id, 0, 0., v(0., 0.), 0., v(0., 0.)),
        [0., 0.]
    );
}

#[test]
fn wrong_stale_foreign_handles_and_invalid_pointers_do_not_write() {
    let mut c = Context::new();
    let mut other = Context::new();
    let id = create(&mut c, [0.1, 0.03, 0.2, 0.007, 0.5]);
    let wrong = c.insert(1_f64).unwrap();
    for (ctx, handle) in [(&mut other, id), (&mut c, wrong), (&mut Context::new(), 0)] {
        let mut out = v(91., 92.);
        assert_eq!(
            unsafe {
                itofin_g2_process_vector(
                    ctx,
                    handle,
                    0,
                    0.,
                    v(0., 0.),
                    0.,
                    v(0., 0.),
                    &mut out,
                    null_mut(),
                )
            },
            INVALID_HANDLE
        );
        assert_eq!(out, v(91., 92.));
    }
    assert_eq!(
        unsafe { crate::boundary::itofin_handle_release(&mut c, id, null_mut()) },
        0
    );
    let mut scalar = 91.;
    assert_eq!(
        unsafe { itofin_g2_process_scalar(&mut c, id, 0, &mut scalar, null_mut()) },
        INVALID_HANDLE
    );
    assert_eq!(scalar, 91.);
    let id = create(&mut c, [0., 0., 0., 0., 0.]);
    assert_eq!(
        unsafe { itofin_g2_process_scalar(&mut c, id, 99, &mut scalar, null_mut()) },
        INVALID_ARGUMENT
    );
    assert_eq!(scalar, 91.);
    assert_eq!(
        unsafe { itofin_g2_process_scalar(&mut c, id, 0, null_mut(), null_mut()) },
        INVALID_ARGUMENT
    );
    assert_eq!(
        unsafe {
            itofin_g2_process_vector(
                &mut c,
                id,
                0,
                0.,
                v(0., 0.),
                0.,
                v(0., 0.),
                null_mut(),
                null_mut(),
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(
        unsafe {
            itofin_g2_process_matrix(&mut c, id, 0, 0., v(0., 0.), 0., null_mut(), null_mut())
        },
        INVALID_ARGUMENT
    );
    let mut out = v(91., 92.);
    assert_eq!(
        unsafe {
            itofin_g2_process_vector(
                null_mut(),
                id,
                0,
                0.,
                v(0., 0.),
                0.,
                v(0., 0.),
                &mut out,
                null_mut(),
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(out, v(91., 92.));
    let misaligned = std::ptr::from_mut(&mut out)
        .cast::<u8>()
        .wrapping_add(1)
        .cast::<ItofinG2Vector>();
    assert_eq!(
        unsafe {
            itofin_g2_process_vector(
                &mut c,
                id,
                0,
                0.,
                v(0., 0.),
                0.,
                v(0., 0.),
                misaligned,
                null_mut(),
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(out, v(91., 92.));
    let error = std::ptr::from_mut(&mut out)
        .cast::<u8>()
        .wrapping_add(1)
        .cast::<ItofinError>();
    assert_eq!(
        unsafe { itofin_g2_process_scalar(&mut c, id, 0, &mut scalar, error) },
        INVALID_ARGUMENT
    );
    assert_eq!(scalar, 91.);
}

#[test]
fn context_thread_checks_precede_any_result_mutation() {
    let mut c = Context::new();
    let id = create(&mut c, [0., 0., 0., 0., 0.]);
    let address = std::ptr::from_mut(&mut c) as usize;
    std::thread::spawn(move || {
        let mut out = v(91., 92.);
        assert_eq!(
            unsafe {
                itofin_g2_process_vector(
                    address as *mut Context,
                    id,
                    0,
                    0.,
                    v(0., 0.),
                    0.,
                    v(0., 0.),
                    &mut out,
                    null_mut(),
                )
            },
            WRONG_THREAD
        );
        assert_eq!(out, v(91., 92.));
    })
    .join()
    .unwrap();
    assert_eq!(
        vec_query(&mut c, id, 0, 0., v(0., 0.), 0., v(0., 0.)),
        [0., 0.]
    );
}
