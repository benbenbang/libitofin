use super::*;

#[test]
fn live_invalid_requests_and_released_handles_leave_outputs_untouched() {
    let mut m = Market::new();
    let mut out = [91.0; 7];
    let mut id = 91;
    let state = [100.0, 0.04];
    unsafe {
        for (kind, len, capacity, code) in [
            (9, 2, 7, INVALID_ARGUMENT),
            (1, 1, 7, INVALID_ARGUMENT),
            (2, 2, 3, INVALID_ARGUMENT),
        ] {
            assert_eq!(
                itofin_gjr_process_query(
                    &mut m.ctx,
                    m.process,
                    kind,
                    0.0,
                    state.as_ptr(),
                    len,
                    out.as_mut_ptr(),
                    capacity,
                    null_mut()
                ),
                code
            );
            assert_eq!(out, [91.0; 7]);
        }
        assert_eq!(
            itofin_gjr_process_query(
                &mut m.ctx,
                m.process,
                1,
                0.0,
                null(),
                2,
                out.as_mut_ptr(),
                7,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_gjr_process_query(
                &mut m.ctx,
                m.process,
                0,
                0.0,
                null(),
                0,
                null_mut(),
                2,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_gjr_process_query(
                null_mut(),
                m.process,
                0,
                0.0,
                null(),
                0,
                out.as_mut_ptr(),
                7,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(out, [91.0; 7]);
        assert_eq!(
            itofin_gjr_process_new(
                &mut m.ctx,
                m.quotes[0],
                m.curves[0],
                m.curves[1],
                &parameters(),
                99,
                &mut id,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(id, 91);
        assert_eq!(
            itofin_gjr_process_new(
                &mut m.ctx,
                m.quotes[0],
                m.curves[0],
                m.curves[1],
                null(),
                0,
                &mut id,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        let bad = ItofinGjrParameters {
            omega: -1.0,
            ..parameters()
        };
        assert_eq!(
            itofin_gjr_process_new(
                &mut m.ctx,
                m.quotes[0],
                m.curves[0],
                m.curves[1],
                &bad,
                0,
                &mut id,
                null_mut()
            ),
            CORE_ERROR
        );
        assert_eq!(id, 91);
        assert_eq!(
            itofin_gjr_process_query(
                &mut m.ctx,
                m.quotes[0],
                0,
                0.0,
                null(),
                0,
                out.as_mut_ptr(),
                7,
                null_mut()
            ),
            INVALID_HANDLE
        );
        let draws = [0.0, f64::NAN];
        assert_eq!(
            itofin_gjr_process_evolve(
                &mut m.ctx,
                m.process,
                0.0,
                state.as_ptr(),
                2,
                0.1,
                draws.as_ptr(),
                2,
                out.as_mut_ptr(),
                7,
                null_mut()
            ),
            CORE_ERROR
        );
        assert_eq!(
            itofin_gjr_process_evolve(
                &mut m.ctx,
                m.process,
                0.0,
                state.as_ptr(),
                usize::MAX,
                0.1,
                draws.as_ptr(),
                2,
                out.as_mut_ptr(),
                7,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(out, [91.0; 7]);
        assert_eq!(itofin_handle_release(&mut m.ctx, m.process, null_mut()), 0);
        assert_eq!(
            itofin_gjr_process_query(
                &mut m.ctx,
                m.process,
                0,
                0.0,
                null(),
                0,
                out.as_mut_ptr(),
                7,
                null_mut()
            ),
            INVALID_HANDLE
        );
        assert_eq!(out, [91.0; 7]);
    }
}

#[test]
fn wrong_thread_context_and_misaligned_error_preserve_output() {
    let mut m = Market::new();
    let address = (&raw mut m.ctx) as usize;
    let process = m.process;
    let (code, out) = std::thread::spawn(move || {
        let mut out = [91.0; 2];
        let code = unsafe {
            itofin_gjr_process_query(
                address as *mut Context,
                process,
                0,
                0.0,
                null(),
                0,
                out.as_mut_ptr(),
                2,
                null_mut(),
            )
        };
        (code, out)
    })
    .join()
    .unwrap();
    assert_eq!(code, itofin_ffi::boundary::WRONG_THREAD);
    assert_eq!(out, [91.0; 2]);
    let mut out = [91.0; 7];
    let mut e = itofin_ffi::ItofinError {
        code: -1,
        message: [0; 1024],
    };
    unsafe {
        let bad_error = (&raw mut e)
            .cast::<u8>()
            .add(1)
            .cast::<itofin_ffi::ItofinError>();
        assert_eq!(
            itofin_gjr_process_query(
                &mut m.ctx,
                m.process,
                0,
                0.0,
                null(),
                0,
                out.as_mut_ptr(),
                7,
                bad_error
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(out, [91.0; 7]);
        assert_eq!(
            itofin_gjr_process_time(&mut m.ctx, m.process, 0, out.as_mut_ptr(), null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(out, [91.0; 7]);
    }
    assert_eq!(m.query(0, &[]), [100.0, 0.04]);
}
