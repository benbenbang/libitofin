use itofin_optimize::{
    Bounds, Common, FireflyOptions, Flow, GlobalOptions, IterationState, Method, MinimizeError,
    Objective, Problem, Termination, minimize,
};
use serde_json::{Value, json};
#[derive(Debug, thiserror::Error)]
enum Failure {
    #[error("objective marker")]
    Objective,
    #[error("callback marker")]
    Callback,
}

struct Probe {
    input: Value,
    calls: Vec<Value>,
    callbacks: Vec<Value>,
    count: usize,
    winner: Option<(Vec<f64>, f64)>,
}

impl Objective for Probe {
    type Error = Failure;

    fn value(&mut self, point: &[f64]) -> Result<f64, Failure> {
        self.count += 1;
        if self.input["fail_at_evaluation"].as_u64() == Some(self.count as u64) {
            self.calls.push(json!({"x":point,"fun":null,"error":true}));
            return Err(Failure::Objective);
        }
        let value = if self.input["nonfinite_at_evaluation"].as_u64() == Some(self.count as u64) {
            f64::NAN
        } else {
            match self.input["objective"].as_str().unwrap() {
                "signed_quadratic" => {
                    (point[0] - 1.25).powi(2) + 4.0 * (point[1] + 0.75).powi(2) - 3.0
                }
                "quadratic" => point[0].powi(2),
                "linear" => point[0],
                "boundary_quadratic" => (point[0] - 3.0).powi(2) - 20.0,
                "constant" | "rounded_normalization" => -7.0,
                "multimodal_polynomial" => {
                    (point[0] - 1.0).powi(2) * ((point[0] + 2.0).powi(2) + 0.1)
                }
                "himmelblau" => {
                    (point[0] * point[0] + point[1] - 11.0).powi(2)
                        + (point[0] + point[1] * point[1] - 7.0).powi(2)
                }
                "shifted_rastrigin" => {
                    15.0 + point
                        .iter()
                        .map(|value| {
                            value * value - 10.0 * (2.0 * std::f64::consts::PI * value).cos()
                        })
                        .sum::<f64>()
                }
                other => panic!("unknown objective {other}"),
            }
        };
        self.calls.push(json!({"x":point, "fun":value}));
        if self
            .winner
            .as_ref()
            .is_none_or(|(_, old)| value.is_finite() && (!old.is_finite() || value < *old))
        {
            self.winner = Some((point.to_vec(), value));
        }
        Ok(value)
    }

    fn gradient(&mut self, _: &[f64], _: &mut [f64]) -> Result<bool, Failure> {
        panic!("firefly must not request a gradient")
    }

    fn callback(&mut self, state: &IterationState<'_>) -> Result<Flow, Failure> {
        self.callbacks.push(json!({"x":state.x,"fun":state.fun,"nit":state.nit,"nfev":state.nfev,"njev":state.njev}));
        if self.input["fail_at_callback"].as_u64() == Some(state.nit as u64) {
            return Err(Failure::Callback);
        }
        Ok(
            if self.input["cancel_after_generation"].as_u64() == Some(state.nit as u64) {
                Flow::Stop
            } else {
                Flow::Continue
            },
        )
    }
}

fn vector(value: &Value) -> Vec<f64> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_f64().unwrap())
        .collect()
}

fn configuration(input: &Value) -> (Problem, FireflyOptions, Common) {
    let bounds = input["bounds"].as_array().unwrap();
    let problem = Problem {
        x0: vector(&input["x0"]),
        bounds: Some(Bounds {
            lower: bounds
                .iter()
                .map(|pair| pair[0].as_f64().unwrap())
                .collect(),
            upper: bounds
                .iter()
                .map(|pair| pair[1].as_f64().unwrap())
                .collect(),
        }),
    };
    let config = &input["options"];
    let mut options = FireflyOptions {
        global: GlobalOptions {
            seed: config["seed"].as_u64().unwrap_or(0),
            population_size: input["population_size"].as_u64().map(|n| n as usize),
            initial_population: input["initial_population"]
                .as_array()
                .map(|rows| rows.iter().map(vector).collect()),
            xatol: config["xatol"].as_f64(),
            fatol: config["fatol"].as_f64(),
        },
        ..Default::default()
    };
    for (name, field) in [
        ("alpha", &mut options.alpha),
        ("beta0", &mut options.beta0),
        ("gamma", &mut options.gamma),
        ("alpha_decay", &mut options.alpha_decay),
    ] {
        if let Some(value) = config[name].as_f64() {
            *field = value;
        }
    }
    let common = Common {
        maxiter: input["maxiter"].as_u64().map(|n| n as usize),
        maxfev: input["maxfev"].as_u64().map(|n| n as usize),
        tol: input["tol"].as_f64(),
    };
    (problem, options, common)
}

fn compare(actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => {
            for (name, value) in expected {
                if !["stage", "row", "target"].contains(&name.as_str()) {
                    compare(&actual[name], value);
                }
            }
        }
        (Value::Array(actual), Value::Array(expected)) => {
            assert_eq!(actual.len(), expected.len());
            for (left, right) in actual.iter().zip(expected) {
                compare(left, right);
            }
        }
        (Value::Number(actual), Value::Number(expected))
            if actual.is_f64() || expected.is_f64() =>
        {
            let actual = actual.as_f64().unwrap();
            let expected = expected.as_f64().unwrap();
            assert!(
                (actual - expected).abs() <= 1e-12 * expected.abs().max(1.0),
                "{actual:?} != {expected:?}"
            );
        }
        _ => assert_eq!(actual, expected),
    }
}

#[test]
fn firefly_matches_independent_traces_errors_and_analytic_quality() {
    let mut fixtures = Vec::<Value>::new();
    let mut references = Value::Null;
    for line in include_str!("fixtures/firefly.jsonl").lines() {
        let record: Value = serde_json::from_str(line).unwrap();
        match record["kind"].as_str().unwrap() {
            "provenance" => references = record["value"]["references"].clone(),
            "attraction" => {}
            "input" => fixtures.push(json!({"input":record["value"],"trace":record["trace"],"expected":{"evaluations":[],"callbacks":[]}})),
            "result" => {
                let fixture = fixtures.last_mut().unwrap();
                assert_eq!(fixture["input"]["name"], record["name"]);
                for (name, value) in record["value"].as_object().unwrap() { fixture["expected"][name] = value.clone(); }
            }
            kind @ ("evaluation" | "callback") => {
                let fixture = fixtures.last_mut().unwrap();
                assert_eq!(fixture["input"]["name"], record["name"]);
                let field = if kind == "evaluation" { "evaluations" } else { "callbacks" };
                fixture["expected"][field].as_array_mut().unwrap().push(record["value"].clone());
            }
            "generation" => {}
            other => panic!("unknown fixture record {other}"),
        }
    }
    assert_eq!(references[0], "https://arxiv.org/html/1003.1466");
    assert_eq!(fixtures.len(), 28);
    for fixture in fixtures {
        let input = &fixture["input"];
        let (problem, options, common) = configuration(input);
        let mut probe = Probe {
            input: input.clone(),
            calls: vec![],
            callbacks: vec![],
            count: 0,
            winner: None,
        };
        let result = minimize(&mut probe, &problem, &Method::Firefly(options), &common);
        let mut actual = match result {
            Ok(result) => {
                assert_eq!(result.nfev, probe.count, "{}", input["name"]);
                assert_eq!(result.nit, probe.callbacks.len());
                let status = match result.status {
                    Termination::Converged(_) => "converged",
                    Termination::MaxIterations => "max_iterations",
                    Termination::MaxEvaluations => "max_evaluations",
                    Termination::Cancelled => "cancelled",
                    Termination::Nonfinite => "nonfinite",
                    other => panic!("unexpected status {other:?}"),
                };
                json!({"x":result.x,"fun":result.fun,"nit":result.nit,"nfev":result.nfev,"njev":result.njev,"status":status,"success":result.success})
            }
            Err(MinimizeError::Objective(error)) => {
                let status = match error {
                    Failure::Objective => "objective_error",
                    Failure::Callback => "callback_error",
                };
                let (point, value) = probe.winner.as_ref().unwrap();
                json!({"x":point,"fun":value,"nit":probe.callbacks.len(),"nfev":probe.count,"njev":0,"status":status,"success":false})
            }
            Err(other) => panic!("unexpected error for {}: {other:?}", input["name"]),
        };
        for call in &probe.calls {
            for (index, value) in vector(&call["x"]).iter().enumerate() {
                let bounds = problem.bounds.as_ref().unwrap();
                assert!(
                    value.is_finite()
                        && *value >= bounds.lower[index]
                        && *value <= bounds.upper[index]
                );
            }
        }
        if fixture["trace"].as_bool().unwrap() {
            actual["evaluations"] = json!(probe.calls);
            actual["callbacks"] = json!(probe.callbacks);
        } else {
            actual["evaluations"] = json!([]);
            actual["callbacks"] = json!([]);
            let target = input["analytic_fun"].as_f64().unwrap();
            let value = actual["fun"].as_f64().unwrap();
            if let Some(gap) = input["global_gap"].as_f64() {
                assert!((value - target - gap).abs() <= 1e-8);
                assert!(value - target > 0.01);
            } else {
                assert!((value - target).abs() <= 1e-8, "{}", input["name"]);
                if !input["analytic_x"].is_null() {
                    for (value, target) in vector(&actual["x"])
                        .iter()
                        .zip(vector(&input["analytic_x"]))
                    {
                        assert!((value - target).abs() <= 1e-4);
                    }
                }
            }
        }
        if fixture["trace"].as_bool().unwrap() {
            compare(&actual, &fixture["expected"]);
            if input["name"] == "microscopic_physical_distance_survives_normalization" {
                let expected = fixture["expected"]["evaluations"].as_array().unwrap();
                for (call, expected) in probe.calls.iter().zip(expected) {
                    assert_eq!(
                        vector(&call["x"])[0].to_bits(),
                        vector(&expected["x"])[0].to_bits()
                    );
                }
            }
        } else {
            assert_eq!(actual["njev"], 0);
            assert_eq!(actual["status"], fixture["expected"]["status"]);
            assert_eq!(actual["success"], fixture["expected"]["success"]);
            assert!(actual["nfev"].as_u64().unwrap() <= common.maxfev.unwrap_or(1_000_000) as u64);
        }
    }
}
