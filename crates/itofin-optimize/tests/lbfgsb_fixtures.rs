//! Compare L-BFGS-B objective quality, feasibility and projected gradient
//! against SciPy, without asserting a shared iterate path or evaluation count.

use itofin_optimize::{Bounds, Common, LbfgsbOptions, Method, Objective, Problem, minimize};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::convert::Infallible;

#[derive(Deserialize)]
struct Fixture {
    provenance: Provenance,
    cases: BTreeMap<String, Case>,
}

#[derive(Deserialize)]
struct Provenance {
    scipy_version: String,
    generated: String,
}

#[derive(Deserialize)]
struct Case {
    x0: Vec<f64>,
    bounds: Vec<[f64; 2]>,
    maxcor: usize,
    fun: f64,
    projected_gnorm: f64,
    status: i64,
}

fn rosenbrock(x: &[f64]) -> (f64, Vec<f64>) {
    let mut f = 0.0;
    let mut gradient = vec![0.0; x.len()];
    for i in 0..x.len() - 1 {
        let residual = x[i + 1] - x[i] * x[i];
        f += 100.0 * residual * residual + (1.0 - x[i]).powi(2);
        gradient[i] += -400.0 * x[i] * residual - 2.0 * (1.0 - x[i]);
        gradient[i + 1] += 200.0 * residual;
    }
    (f, gradient)
}

struct Model {
    bounds: Bounds,
}

impl Objective for Model {
    type Error = Infallible;

    fn value(&mut self, x: &[f64]) -> Result<f64, Self::Error> {
        assert!(
            x.iter()
                .enumerate()
                .all(|(i, &xi)| xi >= self.bounds.lower[i] && xi <= self.bounds.upper[i])
        );
        Ok(rosenbrock(x).0)
    }

    fn gradient(&mut self, x: &[f64], out: &mut [f64]) -> Result<bool, Self::Error> {
        out.copy_from_slice(&rosenbrock(x).1);
        Ok(true)
    }
}

fn projected_norm(x: &[f64], gradient: &[f64], bounds: &Bounds) -> f64 {
    x.iter()
        .zip(gradient)
        .enumerate()
        .fold(0.0_f64, |largest, (i, (&xi, &gi))| {
            let projected = if gi > 0.0 {
                gi.min((xi - bounds.lower[i]).max(0.0))
            } else {
                (-gi).min((bounds.upper[i] - xi).max(0.0))
            };
            largest.max(projected)
        })
}

#[test]
fn cases_match_scipy_quality_and_projected_kkt_residual() {
    let fixture: Fixture = serde_json::from_str(include_str!("fixtures/lbfgsb.json")).unwrap();
    assert!(!fixture.provenance.scipy_version.is_empty());
    assert!(!fixture.provenance.generated.is_empty());
    assert_eq!(fixture.cases.len(), 3);
    for (name, case) in fixture.cases {
        assert_eq!(case.status, 0, "{name}: SciPy did not converge");
        let pairs = if case.bounds.len() == 1 {
            vec![case.bounds[0]; case.x0.len()]
        } else {
            case.bounds.clone()
        };
        let bounds = Bounds {
            lower: pairs.iter().map(|pair| pair[0]).collect(),
            upper: pairs.iter().map(|pair| pair[1]).collect(),
        };
        let mut objective = Model {
            bounds: bounds.clone(),
        };
        let result = minimize(
            &mut objective,
            &Problem {
                x0: case.x0,
                bounds: Some(bounds.clone()),
            },
            &Method::Lbfgsb(LbfgsbOptions {
                maxcor: Some(case.maxcor),
                ftol: Some(1e-12),
                ..LbfgsbOptions::default()
            }),
            &Common::default(),
        )
        .unwrap();
        assert!(result.success, "{name}: {result:?}");
        assert!(
            (result.fun - case.fun).abs() < 1e-6,
            "{name}: Rust {} vs SciPy {}",
            result.fun,
            case.fun
        );
        let residual = projected_norm(&result.x, &rosenbrock(&result.x).1, &bounds);
        assert!(
            residual <= (20.0 * case.projected_gnorm).max(1e-5),
            "{name}: projected gradient {residual} vs SciPy {}: {result:?}",
            case.projected_gnorm
        );
    }
}
