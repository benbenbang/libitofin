use super::*;
use std::ptr::{null, null_mut};

fn create(c: &mut Context) -> u64 {
    let mut id = 91;
    assert_eq!(
        unsafe {
            itofin_mf_state_process_new(
                c,
                0.,
                [0.5, 1.5].as_ptr(),
                2,
                [0.2, 0.4, 0.1].as_ptr(),
                3,
                &mut id,
                null_mut(),
            )
        },
        0
    );
    id
}
fn query(c: &mut Context, id: u64, kind: i32, t: f64, x: f64, dt: f64, dw: f64) -> f64 {
    let mut out = 91.;
    assert_eq!(
        unsafe { itofin_mf_state_process_query(c, id, kind, t, x, dt, dw, &mut out, null_mut()) },
        0
    );
    out
}

#[test]
fn copied_grids_and_all_scalar_queries_delegate_to_core() {
    let mut c = Context::new();
    let mut times = [0.5, 1.5];
    let mut vols = [0.2, 0.4, 0.1];
    let mut id = 91;
    assert_eq!(
        unsafe {
            itofin_mf_state_process_new(
                &mut c,
                -0.2,
                times.as_ptr(),
                2,
                vols.as_ptr(),
                3,
                &mut id,
                null_mut(),
            )
        },
        0
    );
    times[0] = 9.;
    vols[0] = 9.;
    assert_eq!(times[0], 9.);
    assert_eq!(vols[0], 9.);
    let expected =
        MfStateProcess::new(-0.2, vec![0.5, 1.5].into(), vec![0.2, 0.4, 0.1].into()).unwrap();
    for (kind, want) in [
        (0, -0.2),
        (1, 0.),
        (2, 0.),
        (3, 0.4),
        (4, 0.1),
        (5, expected.std_deviation(0.5, 0.1, 2.).unwrap()),
        (6, expected.variance(0.5, 0.1, 2.).unwrap()),
        (7, expected.evolve(0.5, 0.1, 2., -0.7).unwrap()),
    ] {
        assert_eq!(query(&mut c, id, kind, 0.5, 0.1, 2., -0.7), want);
    }
    for (kind, values) in [(0, expected.times()), (1, expected.volatilities())] {
        let mut size = 91;
        assert_eq!(
            unsafe {
                itofin_mf_state_process_grid(&mut c, id, kind, null_mut(), 0, &mut size, null_mut())
            },
            0
        );
        assert_eq!(size, values.len());
        let mut out = vec![91.; size];
        assert_eq!(
            unsafe {
                itofin_mf_state_process_grid(
                    &mut c,
                    id,
                    kind,
                    out.as_mut_ptr(),
                    size,
                    &mut size,
                    null_mut(),
                )
            },
            0
        );
        assert_eq!(out, values);
        out[0] = 9.;
        assert_eq!(query(&mut c, id, 3, 0., 0., 0., 0.), 0.2);
    }
}

#[test]
fn constructor_and_query_errors_leave_outputs_unchanged() {
    let mut c = Context::new();
    for (a, times, vols) in [
        (0., vec![], vec![]),
        (0., vec![1.], vec![0.2]),
        (0., vec![1., 1.], vec![0.2; 3]),
        (0., vec![-1.], vec![0.2; 2]),
        (0., vec![f64::NAN], vec![0.2; 2]),
        (0., vec![], vec![-0.2]),
        (0., vec![], vec![f64::INFINITY]),
        (f64::NAN, vec![], vec![0.2]),
    ] {
        let mut id = 91;
        assert_eq!(
            unsafe {
                itofin_mf_state_process_new(
                    &mut c,
                    a,
                    times.as_ptr(),
                    times.len(),
                    vols.as_ptr(),
                    vols.len(),
                    &mut id,
                    null_mut(),
                )
            },
            CORE_ERROR
        );
        assert_eq!(id, 91);
    }
    let mut id = 91;
    assert_eq!(
        unsafe { itofin_mf_state_process_new(&mut c, 0., null(), 1, &0.2, 1, &mut id, null_mut()) },
        INVALID_ARGUMENT
    );
    assert_eq!(id, 91);
    let id = create(&mut c);
    for (kind, t, x, dt, dw) in [
        (2, -1., 0., 0., 0.),
        (3, 0., f64::NAN, 0., 0.),
        (4, 0., 0., -1., 0.),
        (5, 0., f64::INFINITY, 1., 0.),
        (6, f64::MAX, 0., f64::MAX, 0.),
        (7, 0., 0., 1., f64::NAN),
        (99, 0., 0., 0., 0.),
    ] {
        let mut out = 91.;
        assert_ne!(
            unsafe {
                itofin_mf_state_process_query(&mut c, id, kind, t, x, dt, dw, &mut out, null_mut())
            },
            0
        );
        assert_eq!(out, 91.);
    }
    let mut out = 91.;
    let misaligned = std::ptr::from_mut(&mut out)
        .cast::<u8>()
        .wrapping_add(1)
        .cast::<f64>();
    assert_eq!(
        unsafe {
            itofin_mf_state_process_query(&mut c, id, 1, 0., 0., 0., 0., misaligned, null_mut())
        },
        INVALID_ARGUMENT
    );
    assert_eq!(out, 91.);
    let error = std::ptr::from_mut(&mut out)
        .cast::<u8>()
        .wrapping_add(1)
        .cast::<ItofinError>();
    let mut new_id = 91;
    assert_eq!(
        unsafe { itofin_mf_state_process_new(&mut c, 0., null(), 0, &0.2, 1, &mut new_id, error) },
        INVALID_ARGUMENT
    );
    assert_eq!(new_id, 91);
    assert_eq!(
        unsafe { itofin_mf_state_process_query(&mut c, id, 1, 0., 0., 0., 0., &mut out, error) },
        INVALID_ARGUMENT
    );
    assert_eq!(out, 91.);
}

#[test]
fn grid_failure_and_foreign_wrong_stale_handles_preserve_outputs() {
    let mut c = Context::new();
    let id = create(&mut c);
    let mut out = [91.; 3];
    let mut n = 91;
    for (kind, capacity) in [(0, 1), (1, 2), (99, 3)] {
        assert_eq!(
            unsafe {
                itofin_mf_state_process_grid(
                    &mut c,
                    id,
                    kind,
                    out.as_mut_ptr(),
                    capacity,
                    &mut n,
                    null_mut(),
                )
            },
            INVALID_ARGUMENT
        );
        assert_eq!(out, [91.; 3]);
        assert_eq!(n, 91);
    }
    assert_eq!(
        unsafe { itofin_mf_state_process_grid(&mut c, id, 0, null_mut(), 2, &mut n, null_mut()) },
        INVALID_ARGUMENT
    );
    assert_eq!(n, 91);
    let error = out
        .as_mut_ptr()
        .cast::<u8>()
        .wrapping_add(1)
        .cast::<ItofinError>();
    assert_eq!(
        unsafe { itofin_mf_state_process_grid(&mut c, id, 0, out.as_mut_ptr(), 3, &mut n, error) },
        INVALID_ARGUMENT
    );
    assert_eq!(out, [91.; 3]);
    assert_eq!(n, 91);
    let wrong = c.insert(3_f64).unwrap();
    let mut foreign = Context::new();
    for (ctx, handle) in [(&mut foreign, id), (&mut c, wrong)] {
        assert_eq!(
            unsafe {
                itofin_mf_state_process_query(
                    ctx,
                    handle,
                    1,
                    0.,
                    0.,
                    0.,
                    0.,
                    &mut out[0],
                    null_mut(),
                )
            },
            INVALID_HANDLE
        );
        assert_eq!(
            unsafe {
                itofin_mf_state_process_grid(
                    ctx,
                    handle,
                    0,
                    out.as_mut_ptr(),
                    3,
                    &mut n,
                    null_mut(),
                )
            },
            INVALID_HANDLE
        );
        assert_eq!(out, [91.; 3]);
        assert_eq!(n, 91);
    }
    assert_eq!(unsafe { itofin_handle_release(&mut c, id, null_mut()) }, 0);
    assert_eq!(
        unsafe {
            itofin_mf_state_process_query(&mut c, id, 1, 0., 0., 0., 0., &mut out[0], null_mut())
        },
        INVALID_HANDLE
    );
    assert_eq!(
        unsafe { itofin_mf_state_process_grid(&mut c, id, 0, null_mut(), 0, &mut n, null_mut()) },
        INVALID_HANDLE
    );
    assert_eq!(out, [91.; 3]);
    assert_eq!(n, 91);
}

#[test]
fn wrong_thread_is_rejected_without_reading_or_writing_outputs() {
    let mut c = Context::new();
    let id = create(&mut c);
    let address = std::ptr::from_mut(&mut c) as usize;
    std::thread::spawn(move || {
        let c = address as *mut Context;
        let mut out = 91.;
        let mut n = 91;
        let mut new_id = 91;
        assert_eq!(
            unsafe {
                itofin_mf_state_process_query(c, id, 1, 0., 0., 0., 0., &mut out, null_mut())
            },
            WRONG_THREAD
        );
        assert_eq!(
            unsafe { itofin_mf_state_process_grid(c, id, 0, &mut out, 1, &mut n, null_mut()) },
            WRONG_THREAD
        );
        assert_eq!(
            unsafe {
                itofin_mf_state_process_new(c, 0., null(), 0, &0.2, 1, &mut new_id, null_mut())
            },
            WRONG_THREAD
        );
        assert_eq!(out, 91.);
        assert_eq!(n, 91);
        assert_eq!(new_id, 91);
    })
    .join()
    .unwrap();
}

#[test]
fn required_output_and_grid_alignment_errors_are_atomic() {
    let mut c = Context::new();
    assert_eq!(
        unsafe {
            itofin_mf_state_process_new(&mut c, 0., null(), 0, &0.2, 1, null_mut(), null_mut())
        },
        INVALID_ARGUMENT
    );
    let id = create(&mut c);
    assert_eq!(
        unsafe {
            itofin_mf_state_process_query(&mut c, id, 1, 0., 0., 0., 0., null_mut(), null_mut())
        },
        INVALID_ARGUMENT
    );
    let mut data = [91_f64; 3];
    let mut n = 91;
    assert_eq!(
        unsafe {
            itofin_mf_state_process_grid(
                &mut c,
                id,
                0,
                data.as_mut_ptr(),
                3,
                null_mut(),
                null_mut(),
            )
        },
        INVALID_ARGUMENT
    );
    let bad_count = std::ptr::from_mut(&mut n)
        .cast::<u8>()
        .wrapping_add(1)
        .cast::<usize>();
    assert_eq!(
        unsafe {
            itofin_mf_state_process_grid(&mut c, id, 0, data.as_mut_ptr(), 3, bad_count, null_mut())
        },
        INVALID_ARGUMENT
    );
    let bad_data = data.as_mut_ptr().cast::<u8>().wrapping_add(1).cast::<f64>();
    assert_eq!(
        unsafe { itofin_mf_state_process_grid(&mut c, id, 0, bad_data, 3, &mut n, null_mut()) },
        INVALID_ARGUMENT
    );
    assert_eq!(data, [91.; 3]);
    assert_eq!(n, 91);
}
