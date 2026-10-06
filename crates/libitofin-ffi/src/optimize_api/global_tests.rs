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
