use super::*;
use serde_json::{Value, json};

struct Trace {
    input: Value,
    evaluations: Vec<Value>,
    callbacks: Vec<Value>,
    calls: usize,
    releases: usize,
}

fn number(value: &Value, key: &str) -> usize {
    value[key].as_u64().unwrap_or(0) as usize
}

fn point(value: &Value) -> Vec<f64> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect()
}

fn cost(name: &str, x: &[f64]) -> f64 {
    match name {
        "signed_quadratic" => (x[0] - 1.25).powi(2) + 4.0 * (x[1] + 0.75).powi(2) - 3.0,
        "boundary_quadratic" => (x[0] - 3.0).powi(2) - 20.0,
        "constant" | "rounded_normalization" => -7.0,
        "multimodal_polynomial" => (x[0] - 1.0).powi(2) * ((x[0] + 2.0).powi(2) + 0.1),
        "himmelblau" => (x[0] * x[0] + x[1] - 11.0).powi(2) + (x[0] + x[1] * x[1] - 7.0).powi(2),
        "shifted_rastrigin" => {
            15.0 + x
                .iter()
                .map(|v| v * v - 10.0 * (2.0 * std::f64::consts::PI * v).cos())
                .sum::<f64>()
        }
        _ => panic!("unknown independent objective"),
    }
}

unsafe extern "C" fn evaluate(
    userdata: usize,
    x: *const f64,
    n: usize,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    let trace = unsafe { &mut *(userdata as *mut Trace) };
    trace.calls += 1;
    if trace.calls == number(&trace.input, "fail_at_evaluation") {
        let x = unsafe { std::slice::from_raw_parts(x, n) };
        trace.evaluations.push(json!({"x": x, "fun": null}));
        unsafe {
            (*error).message[0] = b'E' as c_char;
        }
        return 1;
    }
    let x = unsafe { std::slice::from_raw_parts(x, n) };
    let value = if trace.calls == number(&trace.input, "nonfinite_at_evaluation") {
        f64::NAN
    } else {
        cost(trace.input["objective"].as_str().unwrap(), x)
    };
    trace
        .evaluations
        .push(json!({"x": x, "fun": if value.is_finite() { Some(value) } else { None }}));
    unsafe {
        *out = value;
    }
    0
}

unsafe extern "C" fn iteration(
    userdata: usize,
    state: *const ItofinIterationState,
    stop: *mut bool,
    error: *mut ItofinError,
) -> i32 {
    let trace = unsafe { &mut *(userdata as *mut Trace) };
    let state = unsafe { &*state };
    let x = unsafe { std::slice::from_raw_parts(state.x, state.n) };
    trace.callbacks.push(
        json!({"x": x, "fun": state.fun, "nit": state.nit, "nfev": state.nfev, "njev": state.njev}),
    );
    if state.nit == number(&trace.input, "fail_at_callback") {
        unsafe {
            (*error).message[0] = b'C' as c_char;
        }
        return 1;
    }
    unsafe {
        *stop = state.nit == number(&trace.input, "cancel_after_cycle");
    }
    0
}

unsafe extern "C" fn drop_trace(userdata: usize) {
    unsafe {
        (*(userdata as *mut Trace)).releases += 1;
    }
}

fn near(got: f64, want: f64) {
    assert!(
        (got - want).abs() <= 3e-12 * want.abs().max(1.0),
        "{got} != {want}"
    );
}

fn compare_point(got: &Value, want: &Value) {
    let got_x = point(&got["x"]);
    let want_x = point(&want["x"]);
    assert_eq!(got_x.len(), want_x.len());
    for (got, want) in got_x.into_iter().zip(want_x) {
        near(got, want);
    }
    match (got["fun"].as_f64(), want["fun"].as_f64()) {
        (Some(got), Some(want)) => near(got, want),
        (None, None) => (),
        values => panic!("nonfinite mismatch: {values:?}"),
    }
    for key in ["nit", "nfev", "njev"] {
        if !want[key].is_null() {
            assert_eq!(got[key], want[key]);
        }
    }
}

fn options(input: &Value) -> ItofinHybridSimulatedAnnealingOptions {
    let source = &input["options"];
    let mut out = ItofinHybridSimulatedAnnealingOptions {
        seed: source["seed"].as_u64().unwrap_or(0),
        maxiter: number(input, "maxiter"),
        maxfev: number(input, "maxfev"),
        ..Default::default()
    };
    for (value, flag, key) in [
        (&mut out.xatol, &mut out.has_xatol, "xatol"),
        (&mut out.fatol, &mut out.has_fatol, "fatol"),
        (
            &mut out.initial_temperature,
            &mut out.has_initial_temperature,
            "initial_temperature",
        ),
        (
            &mut out.cooling_rate,
            &mut out.has_cooling_rate,
            "cooling_rate",
        ),
        (&mut out.step_size, &mut out.has_step_size, "step_size"),
    ] {
        if let Some(given) = source[key].as_f64() {
            *value = given;
            *flag = true;
        }
    }
    for (value, flag, key) in [
        (
            &mut out.local_search_interval,
            &mut out.has_local_search_interval,
            "local_search_interval",
        ),
        (
            &mut out.local_search_steps,
            &mut out.has_local_search_steps,
            "local_search_steps",
        ),
        (
            &mut out.reanneal_interval,
            &mut out.has_reanneal_interval,
            "reanneal_interval",
        ),
    ] {
        if let Some(given) = source[key].as_u64() {
            *value = given as usize;
            *flag = true;
        }
    }
    out
}

#[test]
fn hybrid_annealing_abi_independent_evaluation_and_callback_fixtures() {
    let records: Vec<Value> = include_str!(
        "../../../../../itofin-optimize/tests/fixtures/hybrid_simulated_annealing.jsonl"
    )
    .lines()
    .map(|line| serde_json::from_str(line).unwrap())
    .collect();
    let mut count = 0;
    for record in records.iter().filter(|r| r["kind"] == "input") {
        count += 1;
        let name = &record["name"];
        let input = record["value"].clone();
        let expected = &records
            .iter()
            .find(|r| r["name"] == *name && r["kind"] == "result")
            .unwrap()["value"];
        let mut trace = Trace {
            input: input.clone(),
            evaluations: vec![],
            callbacks: vec![],
            calls: 0,
            releases: 0,
        };
        let objective = ItofinObjective {
            userdata: &mut trace as *mut Trace as usize,
            value: Some(evaluate),
            callback: Some(iteration),
            release: Some(drop_trace),
            gradient: None,
        };
        let x0 = point(&input["x0"]);
        let bounds = input["bounds"].as_array().unwrap();
        let lower: Vec<_> = bounds.iter().map(|b| b[0].as_f64().unwrap()).collect();
        let upper: Vec<_> = bounds.iter().map(|b| b[1].as_f64().unwrap()).collect();
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
            itofin_optimize_hybrid_simulated_annealing(
                &objective,
                x0.as_ptr(),
                x0.len(),
                lower.as_ptr(),
                lower.len(),
                upper.as_ptr(),
                upper.len(),
                &options(&input),
                &mut result,
                &mut error,
            )
        };
        assert_eq!(trace.releases, 1, "{name}");
        assert_eq!(trace.calls, number(expected, "nfev"), "{name}");
        let expected_status = expected["status"].as_str().unwrap();
        if matches!(expected_status, "objective_error" | "callback_error") {
            assert_eq!(code, CORE_ERROR, "{name}");
            assert_eq!(
                message(&error),
                if expected_status == "objective_error" {
                    "E"
                } else {
                    "C"
                }
            );
            assert_eq!(result.fun, 99.0);
            assert!(x.iter().all(|v| *v == 99.0));
        } else {
            assert_eq!(code, 0, "{name}");
            let actual = json!({"x": x, "fun": if result.fun.is_finite() { Some(result.fun) } else { None }, "nit": result.nit, "nfev": result.nfev, "njev": result.njev});
            compare_point(&actual, expected);
            assert_eq!(result.success, expected["success"].as_bool().unwrap());
            let status = match expected_status {
                "converged" => ITOFIN_OPTIMIZE_CONVERGED_XTOL,
                "max_iterations" => ITOFIN_OPTIMIZE_MAX_ITERATIONS,
                "max_evaluations" => ITOFIN_OPTIMIZE_MAX_EVALUATIONS,
                "cancelled" => ITOFIN_OPTIMIZE_CANCELLED,
                "nonfinite" => ITOFIN_OPTIMIZE_NONFINITE,
                _ => panic!("unknown status"),
            };
            assert_eq!(result.status, status, "{name}");
        }
        if record["trace"] == true {
            for (kind, actual) in [
                ("evaluation", &trace.evaluations),
                ("callback", &trace.callbacks),
            ] {
                let expected: Vec<_> = records
                    .iter()
                    .filter(|r| r["name"] == *name && r["kind"] == kind)
                    .collect();
                assert_eq!(actual.len(), expected.len(), "{name} {kind}");
                for (got, want) in actual.iter().zip(expected) {
                    compare_point(got, &want["value"]);
                }
            }
        }
    }
    assert_eq!(count, 23);
}
