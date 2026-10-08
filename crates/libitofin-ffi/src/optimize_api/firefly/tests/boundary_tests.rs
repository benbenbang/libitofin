use super::*;

#[test]
fn firefly_errors_leave_outputs_untouched_and_release() {
    let mut calls = Calls {
        fail: true,
        ..Calls::default()
    };
    let (code, result, x, error) = run(
        &mut calls,
        &[0.0],
        &[-1.0],
        &[1.0],
        &[],
        0,
        ItofinFireflyOptions::default(),
    );
    assert_eq!(code, CORE_ERROR);
    assert_eq!(message(&error), "E");
    assert_eq!(result.fun, 99.0);
    assert_eq!(result.nfev, 99);
    assert_eq!(result.status, 99);
    assert_eq!(x, vec![99.0]);
    assert_eq!(calls.releases, 1);
}

#[test]
fn firefly_invalid_input_never_evaluates() {
    for (x0, lower, upper, initial, rows) in [
        (vec![0.0], vec![], vec![1.0], vec![], 0),
        (vec![2.0], vec![-1.0], vec![1.0], vec![], 0),
        (vec![0.0], vec![-f64::MAX], vec![f64::MAX], vec![], 0),
        (vec![0.0], vec![-1.0], vec![1.0], vec![0.0; 3], 4),
        (
            vec![0.0],
            vec![-1.0],
            vec![1.0],
            vec![0.0, 0.0, 0.0, f64::NAN],
            4,
        ),
    ] {
        let mut calls = Calls::default();
        let (code, result, x, _) = run(
            &mut calls,
            &x0,
            &lower,
            &upper,
            &initial,
            rows,
            ItofinFireflyOptions::default(),
        );
        assert_eq!(code, INVALID_ARGUMENT);
        assert!(calls.values.is_empty());
        assert_eq!(calls.releases, 1);
        assert_eq!(result.status, 99);
        assert!(x.iter().all(|v| *v == 99.0));
    }
}

#[test]
fn firefly_caps_precede_pointer_reads() {
    for (n, rows, len, population, maxiter, maxfev) in [
        (usize::MAX, usize::MAX, usize::MAX, 0, 0, 0),
        (2, usize::MAX, usize::MAX, 0, 0, 0),
        (1, 4097, 4097, 0, 0, 0),
        (256, 4096, 256 * 4096, 0, 0, 0),
        (1, 0, 0, 4097, 0, 0),
        (1, 0, 0, 0, 1_000_001, 0),
        (1, 0, 0, 0, 0, 10_000_001),
    ] {
        let mut calls = Calls::default();
        let objective = objective(&mut calls);
        let bad = std::ptr::dangling::<f64>();
        let mut options = ItofinFireflyOptions::default();
        options.global.population_size = population;
        options.global.maxiter = maxiter;
        options.global.maxfev = maxfev;
        let mut result = ItofinOptimizeResult {
            x: std::ptr::null_mut(),
            fun: 0.0,
            nit: 0,
            nfev: 0,
            njev: 0,
            status: 0,
            success: false,
        };
        let mut error = blank();
        let code = unsafe {
            itofin_optimize_firefly(
                &objective,
                bad,
                n,
                bad,
                n,
                bad,
                n,
                bad,
                rows,
                len,
                &options,
                &mut result,
                &mut error,
            )
        };
        assert_eq!(code, INVALID_ARGUMENT);
        assert!(calls.values.is_empty());
        assert_eq!(calls.releases, 1);
    }
}

#[test]
fn firefly_coefficients_validate_before_evaluation() {
    for (field, value) in [
        (0, -1.0),
        (0, 1.1),
        (1, 0.0),
        (1, 1.1),
        (2, f64::NAN),
        (2, 1_000_001.0),
        (3, 0.0),
        (3, f64::INFINITY),
    ] {
        let mut options = ItofinFireflyOptions::default();
        match field {
            0 => {
                options.has_alpha = true;
                options.alpha = value;
            }
            1 => {
                options.has_beta0 = true;
                options.beta0 = value;
            }
            2 => {
                options.has_gamma = true;
                options.gamma = value;
            }
            _ => {
                options.has_alpha_decay = true;
                options.alpha_decay = value;
            }
        }
        let mut calls = Calls::default();
        let (code, result, x, _) = run(&mut calls, &[0.0], &[-1.0], &[1.0], &[], 0, options);
        assert_eq!(code, INVALID_ARGUMENT);
        assert!(calls.values.is_empty());
        assert_eq!(calls.releases, 1);
        assert_eq!(result.status, 99);
        assert_eq!(x, [99.0]);
    }
}

#[test]
fn firefly_callback_error_preserves_output_and_releases() {
    let mut calls = Calls {
        callback_fail: true,
        ..Calls::default()
    };
    let (code, result, x, error) = run(
        &mut calls,
        &[0.0],
        &[-1.0],
        &[1.0],
        &[],
        0,
        ItofinFireflyOptions::default(),
    );
    assert_eq!(code, CORE_ERROR);
    assert_eq!(message(&error), "C");
    assert_eq!(result.status, 99);
    assert_eq!(result.fun, 99.0);
    assert_eq!(x, [99.0]);
    assert_eq!(calls.releases, 1);
}
