use super::*;

#[test]
fn hybrid_annealing_objective_and_callback_errors_leave_output_untouched() {
    for callback_fail in [false, true] {
        let mut calls = Calls {
            fail: !callback_fail,
            callback_fail,
            ..Calls::default()
        };
        let (code, result, x, error) = run(&mut calls, &[0.0], &[-1.0], &[1.0], Default::default());
        assert_eq!(code, CORE_ERROR);
        assert_eq!(message(&error), if callback_fail { "C" } else { "E" });
        assert_eq!((result.fun, result.nfev, result.status), (99.0, 99, 99));
        assert_eq!(x, [99.0]);
        assert_eq!(calls.releases, 1);
    }
}

#[test]
fn hybrid_annealing_invalid_input_never_evaluates() {
    for (x0, lower, upper) in [
        (vec![0.0], vec![], vec![1.0]),
        (vec![2.0], vec![-1.0], vec![1.0]),
        (vec![0.0], vec![-f64::MAX], vec![f64::MAX]),
        (vec![f64::NAN], vec![-1.0], vec![1.0]),
        (vec![], vec![], vec![]),
    ] {
        let mut calls = Calls::default();
        let (code, result, x, _) = run(&mut calls, &x0, &lower, &upper, Default::default());
        assert_eq!(code, INVALID_ARGUMENT);
        assert!(calls.values.is_empty());
        assert_eq!(calls.releases, 1);
        assert_eq!(result.status, 99);
        assert!(x.iter().all(|v| *v == 99.0));
    }
}

#[test]
fn hybrid_annealing_invalid_controls_precede_pointer_reads() {
    for field in 0..10 {
        let mut options = ItofinHybridSimulatedAnnealingOptions::default();
        match field {
            0 => options.maxiter = 1_000_001,
            1 => options.maxfev = 10_000_001,
            2 => {
                options.has_initial_temperature = true;
                options.initial_temperature = f64::INFINITY;
            }
            3 => {
                options.has_cooling_rate = true;
                options.cooling_rate = 1.0;
            }
            4 => {
                options.has_step_size = true;
                options.step_size = 0.0;
            }
            5 => options.has_local_search_interval = true,
            6 => {
                options.has_local_search_steps = true;
                options.local_search_steps = 257;
            }
            7 => options.has_reanneal_interval = true,
            8 => {
                options.has_xatol = true;
                options.xatol = -1.0;
            }
            _ => {
                options.has_fatol = true;
                options.fatol = f64::NAN;
            }
        }
        let mut calls = Calls::default();
        let objective = objective(&mut calls);
        let bad = std::ptr::dangling::<f64>();
        let mut result = ItofinOptimizeResult {
            x: std::ptr::null_mut(),
            fun: 99.0,
            nit: 99,
            nfev: 99,
            njev: 99,
            status: 99,
            success: false,
        };
        let mut error = blank();
        let code = unsafe {
            itofin_optimize_hybrid_simulated_annealing(
                &objective,
                bad,
                1,
                bad,
                1,
                bad,
                1,
                &options,
                &mut result,
                &mut error,
            )
        };
        assert_eq!(code, INVALID_ARGUMENT);
        assert_eq!(result.fun, 99.0);
        assert!(calls.values.is_empty());
        assert_eq!(calls.releases, 1);
    }
}

#[test]
fn hybrid_annealing_null_options_select_defaults_and_zero_tolerances_survive() {
    let (decoded, _) = decode_hybrid_simulated_annealing(ItofinHybridSimulatedAnnealingOptions {
        has_xatol: true,
        has_fatol: true,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(decoded.xatol, Some(0.0));
    assert_eq!(decoded.fatol, Some(0.0));
    let mut calls = Calls::default();
    let objective = objective(&mut calls);
    let mut x = [99.0];
    let mut result = ItofinOptimizeResult {
        x: x.as_mut_ptr(),
        fun: 99.0,
        nit: 99,
        nfev: 99,
        njev: 99,
        status: 99,
        success: false,
    };
    let mut error = blank();
    let code = unsafe {
        itofin_optimize_hybrid_simulated_annealing(
            &objective,
            [0.25].as_ptr(),
            1,
            [0.25].as_ptr(),
            1,
            [0.25].as_ptr(),
            1,
            std::ptr::null(),
            &mut result,
            &mut error,
        )
    };
    assert_eq!(code, 0);
    assert_eq!(result.fun, -3.0);
    assert_eq!(calls.releases, 1);
}
