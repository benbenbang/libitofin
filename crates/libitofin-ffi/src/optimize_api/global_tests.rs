use super::*;

#[derive(Default)]
struct Calls {
    values: Vec<Vec<f64>>,
    releases: usize,
    fail: bool,
    stop: bool,
}

unsafe extern "C" fn value(
    userdata: usize,
    x: *const f64,
    n: usize,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    let calls = unsafe { &mut *(userdata as *mut Calls) };
    calls
        .values
        .push(unsafe { std::slice::from_raw_parts(x, n) }.to_vec());
    if calls.fail {
        unsafe {
            (*error).message[0] = b'E' as c_char;
        }
        return 1;
    }
    unsafe {
        *out = std::slice::from_raw_parts(x, n)
            .iter()
            .map(|v| (v - 0.25).powi(2))
            .sum::<f64>()
            - 3.0;
    }
    0
}

unsafe extern "C" fn callback(
    userdata: usize,
    _: *const ItofinIterationState,
    stop: *mut bool,
    _: *mut ItofinError,
) -> i32 {
    unsafe {
        *stop = (*(userdata as *mut Calls)).stop;
    }
    0
}

unsafe extern "C" fn release(userdata: usize) {
    unsafe {
        (*(userdata as *mut Calls)).releases += 1;
    }
}

fn objective(calls: &mut Calls) -> ItofinObjective {
    ItofinObjective {
        userdata: calls as *mut Calls as usize,
        value: Some(value),
        callback: Some(callback),
        release: Some(release),
        gradient: None,
    }
}

fn run(
    calls: &mut Calls,
    x0: &[f64],
    lower: &[f64],
    upper: &[f64],
    initial: &[f64],
    rows: usize,
    options: ItofinDifferentialEvolutionOptions,
) -> (i32, ItofinOptimizeResult, Vec<f64>, ItofinError) {
    let mut x = vec![99.0; x0.len()];
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
        itofin_optimize_differential_evolution(
            &objective(calls),
            x0.as_ptr(),
            x0.len(),
            lower.as_ptr(),
            lower.len(),
            upper.as_ptr(),
            upper.len(),
            initial.as_ptr(),
            rows,
            initial.len(),
            &options,
            &mut result,
            &mut error,
        )
    };
    (code, result, x, error)
}

#[test]
fn differential_evolution_negative_objective_seed_and_counts() {
    let options = ItofinDifferentialEvolutionOptions::default();
    let mut calls = Calls::default();
    let (code, result, x, _) = run(&mut calls, &[0.0], &[-2.0], &[2.0], &[], 0, options);
    assert_eq!(code, 0);
    assert!(result.success);
    assert!((x[0] - 0.25).abs() < 1e-5);
    assert!(result.fun < -2.999999);
    assert_eq!(result.nfev, calls.values.len());
    assert_eq!(result.njev, 0);
    assert_eq!(calls.releases, 1);
    let mut repeat = Calls::default();
    let (code2, result2, x2, _) = run(&mut repeat, &[0.0], &[-2.0], &[2.0], &[], 0, options);
    assert_eq!(code2, 0);
    assert_eq!(x, x2);
    assert_eq!(result.nfev, result2.nfev);
    assert_eq!(calls.values, repeat.values);
}

#[test]
fn differential_evolution_initial_rows_and_explicit_zero() {
    let mut options = ItofinDifferentialEvolutionOptions::default();
    options.global.maxfev = 4;
    options.has_recombination = true;
    options.recombination = 0.0;
    options.global.has_xatol = true;
    options.global.xatol = 0.0;
    let mut calls = Calls::default();
    let (code, result, x, _) = run(
        &mut calls,
        &[0.0],
        &[-1.0],
        &[1.0],
        &[-1.0, 0.25, 0.5, 1.0],
        4,
        options,
    );
    assert_eq!(code, 0);
    assert_eq!(result.status, ITOFIN_OPTIMIZE_MAX_EVALUATIONS);
    assert_eq!(result.nfev, 4);
    assert_eq!(x, vec![0.25]);
    assert_eq!(
        calls.values,
        vec![vec![-1.0], vec![0.25], vec![0.5], vec![1.0]]
    );
    assert_eq!(calls.releases, 1);
}

#[test]
fn differential_evolution_errors_leave_outputs_untouched_and_release() {
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
        ItofinDifferentialEvolutionOptions::default(),
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
fn differential_evolution_invalid_input_never_evaluates() {
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
            ItofinDifferentialEvolutionOptions::default(),
        );
        assert_eq!(code, INVALID_ARGUMENT);
        assert!(calls.values.is_empty());
        assert_eq!(calls.releases, 1);
        assert_eq!(result.status, 99);
        assert!(x.iter().all(|v| *v == 99.0));
    }
}

#[test]
fn differential_evolution_caps_precede_pointer_reads() {
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
        let mut options = ItofinDifferentialEvolutionOptions::default();
        options.global.population_size = population;
        options.global.maxiter = maxiter;
        options.global.maxfev = maxfev;
        let mut result: ItofinOptimizeResult = unsafe { std::mem::zeroed() };
        let mut error = blank();
        let code = unsafe {
            itofin_optimize_differential_evolution(
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
fn differential_evolution_cancelled_returns_partial_result() {
    let mut calls = Calls {
        stop: true,
        ..Calls::default()
    };
    let (code, result, x, _) = run(
        &mut calls,
        &[0.0],
        &[-1.0],
        &[1.0],
        &[],
        0,
        ItofinDifferentialEvolutionOptions::default(),
    );
    assert_eq!(code, 0);
    assert_eq!(result.status, ITOFIN_OPTIMIZE_CANCELLED);
    assert!(!result.success);
    assert_eq!(result.nfev, calls.values.len());
    assert!(x[0].is_finite());
    assert_eq!(calls.releases, 1);
}
