use super::*;
use itofin_ffi::boundary::{CORE_ERROR, INVALID_ARGUMENT, WRONG_THREAD};

#[test]
fn bates_boundaries_leave_outputs_and_model_unchanged() {
    let mut m = Market::new();
    let baseline = m.value();
    let mut id = 999;
    let mut value = 123.0;
    let mut array = [123.0; 8];
    unsafe {
        for (handle, kind, field) in [
            (m.quotes[0], 0, 0),
            (m.process, 1, 0),
            (m.model, 0, 0),
            (m.model, 1, 8),
            (m.model, 2, 0),
        ] {
            assert_ne!(
                itofin_bates_parameter(&mut m.context, handle, kind, field, &mut value, null_mut()),
                0
            );
            assert_eq!(value, 123.0);
        }
        assert_eq!(
            itofin_bates_parameter(&mut m.context, m.process, 0, 0, null_mut(), null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_bates_model_new(&mut m.context, m.quotes[0], &mut id, null_mut()),
            INVALID_HANDLE
        );
        assert_eq!(id, 999);
        for order in [0, 193] {
            assert_eq!(
                itofin_bates_engine_new(&mut m.context, m.model, order, &mut id, null_mut()),
                CORE_ERROR
            );
            assert_eq!(id, 999);
        }
        assert_eq!(
            itofin_bates_process_initial_values(
                &mut m.context,
                m.process,
                array.as_mut_ptr(),
                1,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(array, [123.0; 8]);
        assert_eq!(
            itofin_bates_model_params(&mut m.context, m.model, array.as_mut_ptr(), 7, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(array, [123.0; 8]);
        assert_eq!(
            itofin_bates_model_params(&mut m.context, m.process, array.as_mut_ptr(), 8, null_mut()),
            INVALID_HANDLE
        );
        assert_eq!(array, [123.0; 8]);
        assert_eq!(
            itofin_bates_process_initial_values(
                &mut m.context,
                m.process,
                null_mut(),
                2,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_bates_model_params(&mut m.context, m.model, null_mut(), 8, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_bates_process_time(&mut m.context, m.process, 0, &mut value, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(value, 123.0);
        let mut original = [0.0; 8];
        assert_eq!(
            itofin_bates_model_params(
                &mut m.context,
                m.model,
                original.as_mut_ptr(),
                8,
                null_mut()
            ),
            0
        );
        for (field, bad) in [
            (0, 0.0),
            (1, -1.0),
            (2, f64::NAN),
            (3, 1.01),
            (4, f64::INFINITY),
            (5, 1000.0),
            (6, -0.1),
            (7, -0.1),
        ] {
            let mut params = original;
            params[field] = bad;
            assert_eq!(
                itofin_bates_model_set_params(
                    &mut m.context,
                    m.model,
                    params.as_ptr(),
                    8,
                    null_mut()
                ),
                CORE_ERROR
            );
            assert_eq!(
                itofin_bates_model_params(
                    &mut m.context,
                    m.model,
                    array.as_mut_ptr(),
                    8,
                    null_mut()
                ),
                0
            );
            assert_eq!(array, original);
        }
        assert_eq!(
            itofin_bates_model_set_params(
                &mut m.context,
                m.model,
                original.as_ptr(),
                7,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_bates_model_set_params(&mut m.context, m.model, std::ptr::null(), 8, null_mut()),
            INVALID_ARGUMENT
        );
        let config = parameters();
        assert_eq!(
            itofin_bates_process_new(
                null_mut(),
                m.quotes[0],
                m.curves[0],
                m.curves[1],
                &config,
                &mut id,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_bates_process_new(
                &mut m.context,
                m.quotes[0],
                m.curves[0],
                m.curves[1],
                std::ptr::null(),
                &mut id,
                null_mut()
            ),
            INVALID_ARGUMENT
        );
        let mut foreign = Context::new();
        let foreign_spot = foreign.insert(123_i32).unwrap();
        for spot in [0, m.curves[0], foreign_spot] {
            assert_eq!(
                itofin_bates_process_new(
                    &mut m.context,
                    spot,
                    m.curves[0],
                    m.curves[1],
                    &config,
                    &mut id,
                    null_mut()
                ),
                INVALID_HANDLE
            );
            assert_eq!(id, 999);
        }
    }
    m.set_quote(0, 0.0);
    let mut values = [123.0; 2];
    assert_eq!(
        unsafe {
            itofin_bates_process_initial_values(
                &mut m.context,
                m.process,
                values.as_mut_ptr(),
                2,
                null_mut(),
            )
        },
        CORE_ERROR
    );
    assert_eq!(values, [123.0; 2]);
    m.set_quote(0, 100.0);
    assert_eq!(m.value(), baseline);
    let address = (&mut m.context as *mut Context) as usize;
    let process = m.process;
    let status = std::thread::spawn(move || {
        let mut value = 123.0;
        let status = unsafe {
            itofin_bates_parameter(
                address as *mut Context,
                process,
                0,
                0,
                &mut value,
                null_mut(),
            )
        };
        assert_eq!(value, 123.0);
        status
    })
    .join()
    .unwrap();
    assert_eq!(status, WRONG_THREAD);
    assert_eq!(m.value(), baseline);
}

#[test]
fn misaligned_error_is_rejected_before_any_output_or_mutation() {
    let mut m = Market::new();
    let original = [0.04, 1.5, 0.3, -0.7, 0.04, -0.1, 0.2, 0.5];
    let mut changed = original;
    changed[7] = 0.9;
    let mut id = 999;
    let mut value = 123.0;
    let mut error = std::mem::MaybeUninit::<itofin_ffi::ItofinError>::uninit();
    let misaligned = unsafe { error.as_mut_ptr().cast::<u8>().add(1).cast() };
    unsafe {
        assert_eq!(
            itofin_bates_model_new(&mut m.context, m.process, &mut id, misaligned),
            INVALID_ARGUMENT
        );
        assert_eq!(id, 999);
        assert_eq!(
            itofin_bates_parameter(&mut m.context, m.process, 0, 0, &mut value, misaligned),
            INVALID_ARGUMENT
        );
        assert_eq!(value, 123.0);
        assert_eq!(
            itofin_bates_model_set_params(&mut m.context, m.model, changed.as_ptr(), 8, misaligned),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_ffi::bates_calibration_api::itofin_bates_calibrate_with_options(
                &mut m.context,
                m.model,
                std::ptr::null(),
                0,
                0,
                0,
                144,
                std::ptr::null(),
                misaligned
            ),
            INVALID_ARGUMENT
        );
        let mut actual = [0.0; 8];
        assert_eq!(
            itofin_bates_model_params(&mut m.context, m.model, actual.as_mut_ptr(), 8, null_mut()),
            0
        );
        assert_eq!(actual, original);
    }
}
