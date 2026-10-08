use itofin_optimize::{
    Bounds, Common, Flow, GlobalOptions, IterationState, Method, Objective, ParticleSwarmOptions,
    Problem, Termination, minimize,
};
use serde_json::{Value, json};
use std::convert::Infallible;
struct Probe {
    name: String,
    calls: Vec<Value>,
    callbacks: Vec<Value>,
    cancel: Option<u64>,
}
impl Objective for Probe {
    type Error = Infallible;
    fn value(&mut self, x: &[f64]) -> Result<f64, Infallible> {
        let f = match self.name.as_str() {
            "signed_quadratic" => (x[0] - 1.25).powi(2) + 4.0 * (x[1] + 0.75).powi(2) - 3.0,
            "boundary_quadratic" => (x[0] - 3.0).powi(2) - 20.0,
            "constant" | "rounded_normalization" => -7.0,
            "multimodal_polynomial" => (x[0] - 1.0).powi(2) * ((x[0] + 2.0).powi(2) + 0.1),
            "himmelblau" => {
                (x[0] * x[0] + x[1] - 11.0).powi(2) + (x[0] + x[1] * x[1] - 7.0).powi(2)
            }
            "shifted_rastrigin" => {
                15.0 + x
                    .iter()
                    .map(|v| v * v - 10.0 * (2.0 * std::f64::consts::PI * v).cos())
                    .sum::<f64>()
            }
            _ => panic!("unknown objective"),
        };
        self.calls.push(json!({"x": x,"fun":f}));
        Ok(f)
    }
    fn gradient(&mut self, _: &[f64], _: &mut [f64]) -> Result<bool, Infallible> {
        panic!("gradient must not be requested")
    }
    fn callback(&mut self, s: &IterationState<'_>) -> Result<Flow, Infallible> {
        self.callbacks
            .push(json!({"x":s.x,"fun":s.fun,"nit":s.nit,"nfev":s.nfev,"njev":s.njev}));
        Ok(if self.cancel == Some(s.nit as u64) {
            Flow::Stop
        } else {
            Flow::Continue
        })
    }
}
fn vector(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_f64().unwrap())
        .collect()
}
fn problem_and_options(c: &Value) -> (Problem, ParticleSwarmOptions, Common) {
    let pairs = c["bounds"].as_array().unwrap();
    let problem = Problem {
        x0: vector(&c["x0"]),
        bounds: Some(Bounds {
            lower: pairs.iter().map(|p| p[0].as_f64().unwrap()).collect(),
            upper: pairs.iter().map(|p| p[1].as_f64().unwrap()).collect(),
        }),
    };
    let mut options = ParticleSwarmOptions {
        global: GlobalOptions {
            seed: c["seed"].as_u64().unwrap_or(0),
            population_size: c["population_size"].as_u64().map(|v| v as usize),
            initial_population: c["initial_population"]
                .as_array()
                .map(|r| r.iter().map(vector).collect()),
            xatol: c["xatol"].as_f64(),
            fatol: c["fatol"].as_f64(),
        },
        ..Default::default()
    };
    for (name, field) in [
        ("inertia", &mut options.inertia),
        ("cognitive", &mut options.cognitive),
        ("social", &mut options.social),
        ("velocity_clamp", &mut options.velocity_clamp),
    ] {
        if let Some(v) = c["coefficients"][name].as_f64() {
            *field = v;
        }
    }
    let common = Common {
        maxiter: c["maxiter"].as_u64().map(|v| v as usize),
        maxfev: c["maxfev"].as_u64().map(|v| v as usize),
        tol: None,
    };
    (problem, options, common)
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-12,
        "{actual:?} != {expected:?}"
    );
}

fn compare(actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Object(a), Value::Object(e)) => {
            for (key, value) in e {
                compare(&a[key], value);
            }
        }
        (Value::Array(a), Value::Array(e)) => {
            assert_eq!(a.len(), e.len());
            for (left, right) in a.iter().zip(e) {
                compare(left, right);
            }
        }
        (Value::Number(a), Value::Number(e)) if a.is_f64() || e.is_f64() => {
            close(a.as_f64().unwrap(), e.as_f64().unwrap());
        }
        _ => assert_eq!(actual, expected),
    }
}

#[test]
fn particle_swarm_matches_independent_policy_traces_and_analytic_quality() {
    let mut fixtures = json!({"cases": []});
    for line in include_str!("fixtures/particle_swarm.jsonl").lines() {
        let record: Value = serde_json::from_str(line).unwrap();
        let cases = fixtures["cases"].as_array_mut().unwrap();
        match record["kind"].as_str().unwrap() {
            "provenance" => fixtures["provenance"] = record["value"].clone(),
            "input" => cases.push(json!({"input": record["value"], "trace":record["trace"],
                                        "expected": {"evaluations":[],"callbacks":[]}})),
            "result" => {
                let case = cases.last_mut().unwrap();
                assert_eq!(case["input"]["name"], record["name"]);
                for (key, value) in record["value"].as_object().unwrap() {
                    case["expected"][key] = value.clone();
                }
            }
            kind @ ("evaluation" | "callback") => {
                let case = cases.last_mut().unwrap();
                assert_eq!(case["input"]["name"], record["name"]);
                let field = if kind == "evaluation" {
                    "evaluations"
                } else {
                    "callbacks"
                };
                case["expected"][field]
                    .as_array_mut()
                    .unwrap()
                    .push(record["value"].clone());
            }
            _ => panic!("unknown fixture record"),
        }
    }
    assert_eq!(
        fixtures["provenance"]["references"][0],
        "https://doi.org/10.1109/ICNN.1995.488968"
    );
    assert_eq!(fixtures["cases"].as_array().unwrap().len(), 17);
    for fixture in fixtures["cases"].as_array().unwrap() {
        let c = &fixture["input"];
        let (problem, options, common) = problem_and_options(c);
        let mut objective = Probe {
            name: c["objective"].as_str().unwrap().into(),
            calls: vec![],
            callbacks: vec![],
            cancel: c["cancel_after_generation"].as_u64(),
        };
        let result = minimize(
            &mut objective,
            &problem,
            &Method::ParticleSwarm(options),
            &common,
        )
        .unwrap();
        let status = match result.status {
            Termination::Converged(_) => "converged",
            Termination::MaxIterations => "max_iterations",
            Termination::MaxEvaluations => "max_evaluations",
            Termination::Cancelled => "cancelled",
            other => panic!("unexpected {other:?}"),
        };
        assert_eq!(result.nfev, objective.calls.len(), "{}", c["name"]);
        assert_eq!(result.nit, objective.callbacks.len(), "{}", c["name"]);
        assert_eq!(result.njev, 0);
        let bounds = c["bounds"].as_array().unwrap();
        for call in &objective.calls {
            for (x, bound) in vector(&call["x"]).iter().zip(bounds) {
                assert!(*x >= bound[0].as_f64().unwrap() && *x <= bound[1].as_f64().unwrap());
            }
        }
        let actual = json!({"x":result.x,"fun":result.fun,"nit":result.nit,"nfev":result.nfev,
                            "njev":result.njev,"status":status,"success":result.success,
                            "callbacks":objective.callbacks,"evaluations":objective.calls});
        if fixture["trace"].as_bool().unwrap() {
            compare(&actual, &fixture["expected"]);
            if [
                "zero_coefficients",
                "rounded_normalization_does_not_converge",
            ]
            .contains(&c["name"].as_str().unwrap())
            {
                let rows = c["initial_population"].as_array().unwrap();
                for (i, call) in objective.calls.iter().enumerate() {
                    for (a, e) in vector(&call["x"]).iter().zip(vector(&rows[i % rows.len()])) {
                        assert_eq!(a.to_bits(), e.to_bits());
                    }
                }
            }
        } else {
            close(result.fun, c["analytic_fun"].as_f64().unwrap());
            let minima = c["minima"]
                .as_array()
                .map(|m| m.iter().map(vector).collect::<Vec<_>>())
                .unwrap_or_else(|| vec![vector(&c["analytic_x"])]);
            assert!(
                minima.iter().any(|point| result
                    .x
                    .iter()
                    .zip(point)
                    .all(|(a, e)| (a - e).abs() <= 1e-4)),
                "{}: {:?}",
                c["name"],
                result.x
            );
            assert!(matches!(
                result.status,
                Termination::Converged(_) | Termination::MaxIterations
            ));
            assert_eq!(result.nfev, 64 * (result.nit + 1));
        }
    }
}
