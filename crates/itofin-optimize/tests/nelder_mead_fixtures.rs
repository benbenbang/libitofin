//! Compares the Nelder-Mead solver against the SciPy reference fixtures.
//!
//! The oracle is the optimum, never the path to it: a case passes when both
//! implementations report convergence and land on the same point and value.
//! Iterates, `nit` and `nfev` are deliberately not compared, because matching
//! them would test SciPy's order of arithmetic rather than this solver.

use itofin_optimize::{
    Common, Converged, Method, NelderMeadOptions, Problem, Termination, minimize,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::convert::Infallible;

/// The tolerance on the objective value at the reported optimum.
const VALUE_TOLERANCE: f64 = 1e-6;

/// The tolerance on each coordinate of the reported optimum.
const POINT_TOLERANCE: f64 = 1e-3;

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
    options: Options,
    x: Vec<f64>,
    fun: f64,
}

#[derive(Deserialize)]
struct Options {
    xatol: f64,
    fatol: f64,
    maxiter: usize,
    maxfev: usize,
    #[serde(default)]
    adaptive: bool,
}

fn rosenbrock(x: &[f64]) -> f64 {
    100.0 * (x[1] - x[0] * x[0]).powi(2) + (1.0 - x[0]).powi(2)
}

fn sphere(x: &[f64]) -> f64 {
    x.iter().map(|value| value * value).sum()
}

fn beale(x: &[f64]) -> f64 {
    (1.5 - x[0] + x[0] * x[1]).powi(2)
        + (2.25 - x[0] + x[0] * x[1] * x[1]).powi(2)
        + (2.625 - x[0] + x[0] * x[1].powi(3)).powi(2)
}

fn quadratic(x: &[f64]) -> f64 {
    let (a, b, c, d) = (x[0] - 1.0, x[1] + 2.0, x[2] - 0.5, x[3] + 1.0);
    a * a + 2.0 * b * b + 3.0 * c * c + 4.0 * d * d + 0.5 * a * b + 0.25 * b * c + 0.1 * c * d
}

/// The objective each case is named after, defined here rather than carried in
/// the JSON so that the fixture file stays small and readable.
fn objective(name: &str) -> fn(&[f64]) -> f64 {
    match name {
        "rosenbrock" => rosenbrock,
        "sphere5" | "adaptive_sphere10" => sphere,
        "beale" => beale,
        "quadratic4" => quadratic,
        other => panic!("fixture case {other} has no objective"),
    }
}

fn load() -> Fixture {
    serde_json::from_str(include_str!("fixtures/nelder_mead.json"))
        .expect("the fixture file is valid JSON in the expected shape")
}

#[test]
fn the_fixtures_record_the_scipy_release_that_produced_them() {
    let fixture = load();
    assert!(!fixture.provenance.scipy_version.is_empty());
    assert!(!fixture.provenance.generated.is_empty());
    assert_eq!(fixture.cases.len(), 5);
}

#[test]
fn every_case_reaches_the_optimum_scipy_reached() {
    let fixture = load();
    for (name, case) in &fixture.cases {
        let function = objective(name);
        let mut wrapped = move |x: &[f64]| -> Result<f64, Infallible> { Ok(function(x)) };
        let problem = Problem {
            x0: case.x0.clone(),
            bounds: None,
        };
        let options = NelderMeadOptions {
            xatol: Some(case.options.xatol),
            fatol: Some(case.options.fatol),
            adaptive: case.options.adaptive,
            initial_simplex: None,
        };
        let common = Common {
            maxiter: Some(case.options.maxiter),
            maxfev: Some(case.options.maxfev),
            tol: None,
        };
        let result = minimize(
            &mut wrapped,
            &problem,
            &Method::NelderMead(options),
            &common,
        )
        .expect("an infallible objective cannot fail");

        assert_eq!(
            result.status,
            Termination::Converged(Converged::XTol),
            "{name} did not converge"
        );
        assert!(
            (result.fun - case.fun).abs() <= VALUE_TOLERANCE,
            "{name}: fun {} against {}",
            result.fun,
            case.fun
        );
        for (ours, theirs) in result.x.iter().zip(&case.x) {
            assert!(
                (ours - theirs).abs() <= POINT_TOLERANCE,
                "{name}: x {:?} against {:?}",
                result.x,
                case.x
            );
        }
    }
}
