//! Compares the BFGS solver against the SciPy reference fixtures.
//!
//! The oracle is the optimum, never the path to it: a case passes when both
//! implementations report convergence, reach the same objective value and
//! leave a gradient residual within the default `gtol`. Iterates, `nit`,
//! `nfev` and `njev` are deliberately not compared.

use itofin_optimize::{
    BfgsOptions, Common, Converged, Method, Objective, Problem, Termination, minimize,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::convert::Infallible;

/// The tolerance on the objective value at the reported optimum.
const VALUE_TOLERANCE: f64 = 1e-6;

/// SciPy's default `gtol`, which both sides run with.
const GTOL: f64 = 1e-5;

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
    analytic_gradient: bool,
    fun: f64,
    gnorm: f64,
    status: i64,
}

fn rosenbrock(x: &[f64]) -> (f64, Vec<f64>) {
    let r = x[1] - x[0] * x[0];
    let f = 100.0 * r * r + (1.0 - x[0]).powi(2);
    (f, vec![-400.0 * x[0] * r - 2.0 * (1.0 - x[0]), 200.0 * r])
}

fn beale(x: &[f64]) -> (f64, Vec<f64>) {
    let (a, b, c) = (
        1.5 - x[0] + x[0] * x[1],
        2.25 - x[0] + x[0] * x[1] * x[1],
        2.625 - x[0] + x[0] * x[1].powi(3),
    );
    let g0 = 2.0 * (a * (x[1] - 1.0) + b * (x[1] * x[1] - 1.0) + c * (x[1].powi(3) - 1.0));
    let g1 = 2.0 * x[0] * (a + 2.0 * b * x[1] + 3.0 * c * x[1] * x[1]);
    (a * a + b * b + c * c, vec![g0, g1])
}

fn powell_singular(x: &[f64]) -> (f64, Vec<f64>) {
    let (a, b, c, d) = (
        x[0] + 10.0 * x[1],
        x[2] - x[3],
        x[1] - 2.0 * x[2],
        x[0] - x[3],
    );
    let f = a * a + 5.0 * b * b + c.powi(4) + 10.0 * d.powi(4);
    let g = vec![
        2.0 * a + 40.0 * d.powi(3),
        20.0 * a + 4.0 * c.powi(3),
        10.0 * b - 8.0 * c.powi(3),
        -10.0 * b - 40.0 * d.powi(3),
    ];
    (f, g)
}

/// The objective each case is named after, defined here rather than carried in
/// the JSON so that the fixture file stays small and readable.
fn function(name: &str) -> fn(&[f64]) -> (f64, Vec<f64>) {
    match name {
        "rosenbrock" => rosenbrock,
        "beale" => beale,
        "powell_singular" => powell_singular,
        other => panic!("fixture case {other} has no objective"),
    }
}

/// A case objective that hands over its analytic gradient only when SciPy was
/// given one, so both sides approximate or both sides do not.
struct CaseObjective {
    function: fn(&[f64]) -> (f64, Vec<f64>),
    analytic: bool,
}

impl Objective for CaseObjective {
    type Error = Infallible;

    fn value(&mut self, x: &[f64]) -> Result<f64, Self::Error> {
        Ok((self.function)(x).0)
    }

    fn gradient(&mut self, x: &[f64], out: &mut [f64]) -> Result<bool, Self::Error> {
        if self.analytic {
            out.copy_from_slice(&(self.function)(x).1);
        }
        Ok(self.analytic)
    }
}

fn load() -> Fixture {
    serde_json::from_str(include_str!("fixtures/bfgs.json"))
        .expect("the fixture file is valid JSON in the expected shape")
}

#[test]
fn the_fixtures_record_the_scipy_release_that_produced_them() {
    let fixture = load();
    assert!(!fixture.provenance.scipy_version.is_empty());
    assert!(!fixture.provenance.generated.is_empty());
    assert_eq!(fixture.cases.len(), 3);
}

#[test]
fn every_case_reaches_the_optimum_scipy_reached() {
    let fixture = load();
    for (name, case) in &fixture.cases {
        assert_eq!(case.status, 0, "{name}: SciPy did not converge");
        assert!(case.gnorm <= GTOL, "{name}: SciPy residual {}", case.gnorm);
        let mut objective = CaseObjective {
            function: function(name),
            analytic: case.analytic_gradient,
        };
        let problem = Problem {
            x0: case.x0.clone(),
            bounds: None,
        };
        let method = Method::Bfgs(BfgsOptions::default());
        let result = minimize(&mut objective, &problem, &method, &Common::default())
            .expect("an infallible objective cannot fail");

        assert_eq!(
            result.status,
            Termination::Converged(Converged::GTol),
            "{name} did not converge"
        );
        assert!(
            (result.fun - case.fun).abs() <= VALUE_TOLERANCE,
            "{name}: fun {} against {}",
            result.fun,
            case.fun
        );
        let gradient = (objective.function)(&result.x).1;
        let residual = gradient
            .iter()
            .fold(0.0_f64, |largest, g| g.abs().max(largest));
        assert!(residual <= GTOL, "{name}: residual {residual}");
    }
}
