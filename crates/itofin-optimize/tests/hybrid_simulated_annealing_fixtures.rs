use itofin_optimize::{
    Bounds, Common, Flow, HybridSimulatedAnnealingOptions, IterationState, Method, MinimizeError,
    Objective, Problem, Termination, minimize,
};
use serde_json::{Value, json};
use std::io;

struct Probe {
    input: Value,
    calls: Vec<Value>,
    callbacks: Vec<Value>,
    count: usize,
    winner: Option<(Vec<f64>, f64)>,
}

impl Objective for Probe {
    type Error = io::Error;

    fn value(&mut self, point: &[f64]) -> Result<f64, io::Error> {
        self.count += 1;
        if self.input["fail_at_evaluation"].as_u64() == Some(self.count as u64) {
            self.calls.push(json!({"x":point,"fun":null,"error":true}));
            return Err(io::Error::other("objective marker"));
        }
        let value = if self.input["nonfinite_at_evaluation"].as_u64() == Some(self.count as u64) {
            f64::NAN
        } else {
            match self.input["objective"].as_str().unwrap() {
                "signed_quadratic" => {
                    (point[0] - 1.25).powi(2) + 4.0 * (point[1] + 0.75).powi(2) - 3.0
                }
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

    fn gradient(&mut self, _: &[f64], _: &mut [f64]) -> Result<bool, io::Error> {
        panic!("hybrid annealing must not request a gradient")
    }

    fn callback(&mut self, state: &IterationState<'_>) -> Result<Flow, io::Error> {
        self.callbacks.push(json!({"x":state.x,"fun":state.fun,"nit":state.nit,"nfev":state.nfev,"njev":state.njev}));
        if self.input["fail_at_callback"].as_u64() == Some(state.nit as u64) {
            return Err(io::Error::other("callback marker"));
        }
        Ok(
            if self.input["cancel_after_cycle"].as_u64() == Some(state.nit as u64) {
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

fn configuration(input: &Value) -> (Problem, HybridSimulatedAnnealingOptions, Common) {
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
    let mut options = HybridSimulatedAnnealingOptions {
        seed: config["seed"].as_u64().unwrap_or(0),
        xatol: config["xatol"].as_f64(),
        fatol: config["fatol"].as_f64(),
        ..Default::default()
    };
    for (name, field) in [
        ("initial_temperature", &mut options.initial_temperature),
        ("cooling_rate", &mut options.cooling_rate),
        ("step_size", &mut options.step_size),
    ] {
        if let Some(value) = config[name].as_f64() {
            *field = value;
        }
    }
    for (name, field) in [
        ("local_search_interval", &mut options.local_search_interval),
        ("local_search_steps", &mut options.local_search_steps),
        ("reanneal_interval", &mut options.reanneal_interval),
    ] {
        if let Some(value) = config[name].as_u64() {
            *field = value as usize;
        }
    }
    let common = Common {
        maxiter: input["maxiter"].as_u64().map(|value| value as usize),
        maxfev: input["maxfev"].as_u64().map(|value| value as usize),
        tol: None,
    };
    (problem, options, common)
}

fn compare(actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => {
            for (name, value) in expected {
                if name != "stage" {
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
fn hybrid_annealing_matches_independent_traces_errors_and_analytic_quality() {
    let mut fixtures = Vec::<Value>::new();
    let mut references = Value::Null;
    for line in include_str!("fixtures/hybrid_simulated_annealing.jsonl").lines() {
        let record: Value = serde_json::from_str(line).unwrap();
        match record["kind"].as_str().unwrap() {
            "provenance" => references = record["value"]["references"].clone(),
            "acceptance" => {}
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
            "cycle" => {}
            other => panic!("unknown fixture record {other}"),
        }
    }
    assert_eq!(
        references[0],
        "https://doi.org/10.1126/science.220.4598.671"
    );
    assert_eq!(fixtures.len(), 23);
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
        let result = minimize(
            &mut probe,
            &problem,
            &Method::HybridSimulatedAnnealing(options),
            &common,
        );
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
                let status = match error.to_string().as_str() {
                    "objective marker" => "objective_error",
                    "callback marker" => "callback_error",
                    other => panic!("unexpected typed error {other}"),
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
                assert!(value - target > 0.99);
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
        compare(&actual, &fixture["expected"]);
    }
}
