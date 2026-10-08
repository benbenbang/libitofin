use super::*;

#[derive(Default)]
struct Calls {
    values: Vec<Vec<f64>>,
    releases: usize,
    gradients: usize,
    fail: bool,
    stop: bool,
    callback_fail: bool,
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
    error: *mut ItofinError,
) -> i32 {
    if unsafe { (*(userdata as *mut Calls)).callback_fail } {
        unsafe {
            (*error).message[0] = b'C' as c_char;
        }
        return 1;
    }
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

unsafe extern "C" fn unused_gradient(
    userdata: usize,
    _: *const f64,
    _: usize,
    _: *mut f64,
    _: *mut ItofinError,
) -> i32 {
    unsafe {
        (*(userdata as *mut Calls)).gradients += 1;
    }
    1
}

fn objective(calls: &mut Calls) -> ItofinObjective {
    ItofinObjective {
        userdata: calls as *mut Calls as usize,
        value: Some(value),
        callback: Some(callback),
        release: Some(release),
        gradient: Some(unused_gradient),
    }
}

fn run(
    calls: &mut Calls,
    x0: &[f64],
    lower: &[f64],
    upper: &[f64],
    initial: &[f64],
    rows: usize,
    options: ItofinParticleSwarmOptions,
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
        itofin_optimize_particle_swarm(
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
fn particle_swarm_negative_objective_seed_and_counts() {
    let options = ItofinParticleSwarmOptions::default();
    let mut calls = Calls::default();
    let (code, result, x, _) = run(&mut calls, &[0.0], &[-2.0], &[2.0], &[], 0, options);
    assert_eq!(code, 0);
    assert!(result.success);
    assert!((x[0] - 0.25).abs() < 1e-5);
    assert!(result.fun < -2.999999);
    assert_eq!(result.nfev, calls.values.len());
    assert_eq!(result.njev, 0);
    assert_eq!(calls.gradients, 0);
    assert_eq!(calls.releases, 1);
    let mut repeat = Calls::default();
    let (code2, result2, x2, _) = run(&mut repeat, &[0.0], &[-2.0], &[2.0], &[], 0, options);
    assert_eq!(code2, 0);
    assert_eq!(x, x2);
    assert_eq!(result.nfev, result2.nfev);
    assert_eq!(calls.values, repeat.values);
}

#[test]
fn particle_swarm_initial_rows_and_explicit_zero() {
    let mut options = ItofinParticleSwarmOptions::default();
    options.global.maxfev = 4;
    options.has_cognitive = true;
    options.cognitive = 0.0;
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
fn particle_swarm_cancelled_returns_partial_result() {
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
        ItofinParticleSwarmOptions::default(),
    );
    assert_eq!(code, 0);
    assert_eq!(result.status, ITOFIN_OPTIMIZE_CANCELLED);
    assert!(!result.success);
    assert_eq!(result.nfev, calls.values.len());
    assert!(x[0].is_finite());
    assert_eq!(calls.releases, 1);
}

#[test]
fn particle_swarm_all_fixed_uses_one_value_and_no_gradient() {
    let mut calls = Calls::default();
    let (code, result, x, _) = run(
        &mut calls,
        &[0.25],
        &[0.25],
        &[0.25],
        &[],
        0,
        ItofinParticleSwarmOptions::default(),
    );
    assert_eq!(code, 0);
    assert!(result.success);
    assert_eq!(result.fun, -3.0);
    assert_eq!((result.nit, result.nfev, result.njev), (0, 1, 0));
    assert_eq!(x, [0.25]);
    assert_eq!(calls.releases, 1);
    assert_eq!(calls.values, [vec![0.25]]);
}

#[test]
fn particle_swarm_partial_generation_counts_and_zero_coefficients() {
    let mut options = ItofinParticleSwarmOptions::default();
    options.global.population_size = 4;
    options.global.maxfev = 5;
    options.has_inertia = true;
    options.has_cognitive = true;
    options.has_social = true;
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
    assert_eq!((result.nit, result.nfev), (0, 5));
    assert_eq!(result.fun, -3.0);
    assert_eq!(x, [0.25]);
    assert_eq!(calls.values[0], calls.values[4]);
    assert_eq!(calls.releases, 1);
}
