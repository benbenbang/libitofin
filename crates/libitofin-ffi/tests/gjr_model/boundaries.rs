use super::*;
use itofin_ffi::ItofinError;
use itofin_ffi::boundary::{CORE_ERROR, INVALID_ARGUMENT, INVALID_HANDLE, WRONG_THREAD};

#[test]
fn invalid_parameters_buffers_handles_and_errors_are_atomic() {
    let mut m = Market::new();
    let original = m.params();
    unsafe {
        for capacity in [0, 5, usize::MAX] {
            let mut out = [-999.0; 6];
            assert_eq!(
                itofin_gjr_model_params(
                    &mut m.context,
                    m.model,
                    out.as_mut_ptr(),
                    capacity,
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(out, [-999.0; 6]);
        }
        for len in [0, 5, 7, usize::MAX] {
            assert_eq!(
                itofin_gjr_model_set_params(
                    &mut m.context,
                    m.model,
                    original.as_ptr(),
                    len,
                    null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(m.params(), original);
        }
        for (index, value) in [
            (0, -1.0),
            (1, f64::NAN),
            (2, 1.1),
            (3, -0.2),
            (4, f64::INFINITY),
            (5, 0.0),
        ] {
            let mut bad = original;
            bad[index] = value;
            assert_eq!(
                itofin_gjr_model_set_params(&mut m.context, m.model, bad.as_ptr(), 6, null_mut()),
                CORE_ERROR
            );
            assert_eq!(m.params(), original);
        }
        let mut id = 999;
        assert_eq!(
            itofin_gjr_model_new(&mut m.context, m.model, &mut id, null_mut()),
            INVALID_HANDLE
        );
        assert_eq!(id, 999);
        assert_eq!(
            itofin_gjr_model_process(&mut m.context, m.process, &mut id, null_mut()),
            INVALID_HANDLE
        );
        assert_eq!(id, 999);
        assert_eq!(
            itofin_gjr_analytic_engine_new(&mut m.context, m.process, &mut id, null_mut()),
            INVALID_HANDLE
        );
        let mut foreign = Context::new();
        let mut out = [-999.0; 6];
        assert_eq!(
            itofin_gjr_model_params(&mut foreign, m.model, out.as_mut_ptr(), 6, null_mut()),
            INVALID_HANDLE
        );
        assert_eq!(out, [-999.0; 6]);
        let mut storage = [0_u64; 140];
        let misaligned_error = storage
            .as_mut_ptr()
            .cast::<u8>()
            .add(1)
            .cast::<ItofinError>();
        assert_eq!(
            itofin_gjr_model_new(&mut m.context, m.process, &mut id, misaligned_error),
            INVALID_ARGUMENT
        );
        assert_eq!(id, 999);
        let mut changed = original;
        changed[5] *= 1.1;
        assert_eq!(
            itofin_gjr_model_set_params(
                &mut m.context,
                m.model,
                changed.as_ptr(),
                6,
                misaligned_error
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(m.params(), original);
        let misaligned = storage.as_mut_ptr().cast::<u8>().add(1).cast::<f64>();
        assert_eq!(
            itofin_gjr_model_params(&mut m.context, m.model, misaligned, 6, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_gjr_model_set_params(&mut m.context, m.model, misaligned, 6, null_mut()),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_gjr_model_set_params(&mut m.context, m.model, std::ptr::null(), 6, null_mut()),
            INVALID_ARGUMENT
        );
    }
    let ctx = (&mut m.context as *mut Context) as usize;
    let model = m.model;
    let result = std::thread::spawn(move || unsafe {
        let mut out = [-999.0; 6];
        let result =
            itofin_gjr_model_params(ctx as *mut Context, model, out.as_mut_ptr(), 6, null_mut());
        assert_eq!(out, [-999.0; 6]);
        result
    })
    .join()
    .unwrap();
    assert_eq!(result, WRONG_THREAD);
    assert_eq!(m.params(), original);
}

#[test]
fn unavailable_greeks_and_american_pricing_leave_outputs_unchanged() {
    let mut m = Market::new();
    let baseline = m.value();
    for field in 1..=7 {
        let mut value = -999.0;
        assert_eq!(
            unsafe { itofin_option_value(&mut m.context, m.option, field, &mut value, null_mut()) },
            CORE_ERROR
        );
        assert_eq!(value, -999.0);
    }
    let today = Date::new(3, Month::October, 2026);
    let settings = shared(Settings::<Date>::new());
    settings.set_evaluation_date(today);
    let settings = m.context.insert(settings).unwrap();
    let mut american = 0;
    assert_eq!(
        unsafe {
            itofin_option_new(
                &mut m.context,
                0,
                100.0,
                today.serial_number(),
                today.serial_number() + 365,
                1,
                settings,
                &mut american,
                null_mut(),
            )
        },
        0
    );
    assert_eq!(
        unsafe { itofin_option_set_engine(&mut m.context, american, m.engine, 2, 0, null_mut()) },
        0
    );
    let mut value = -999.0;
    assert_eq!(
        unsafe { itofin_option_value(&mut m.context, american, 0, &mut value, null_mut()) },
        CORE_ERROR
    );
    assert_eq!(value, -999.0);
    assert_eq!(m.value(), baseline);
}
