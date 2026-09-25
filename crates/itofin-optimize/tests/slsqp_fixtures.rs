//! Compares the SLSQP solver against the SciPy reference fixtures.
//!
//! The oracle is the optimum and its optimality conditions, never the path to
//! it: a case passes when this solver converges to SciPy's objective value, is
//! at least as feasible, and meets the KKT stationarity condition as tightly.
//! Iterates, `nit` and `nfev` are deliberately not compared.

use itofin_optimize::{
    Bounds, Common, ConstraintKind, Converged, Method, Objective, Problem, SlsqpOptions,
    Termination, minimize,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::convert::Infallible;

/// The tolerance on the objective value at the reported optimum.
const VALUE_TOLERANCE: f64 = 1e-9;

/// The largest constraint violation accepted at the reported optimum.
const VIOLATION_TOLERANCE: f64 = 1e-8;

/// The KKT residual accepted beyond SciPy's own, which already carries the
/// error of the central differences both sides use.
const KKT_TOLERANCE: f64 = 1e-5;

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
    bounds: Option<Vec<[Option<f64>; 2]>>,
    ftol: f64,
    fun: f64,
    max_violation: f64,
    kkt_residual: f64,
}

type Function = fn(&[f64]) -> f64;

struct Constrained {
    f: Function,
    constraints: Vec<(ConstraintKind, Function)>,
}

impl Objective for Constrained {
    type Error = Infallible;

    fn value(&mut self, x: &[f64]) -> Result<f64, Self::Error> {
        Ok((self.f)(x))
    }

    fn constraint_count(&self) -> usize {
        self.constraints.len()
    }

    fn constraint_kind(&self, i: usize) -> ConstraintKind {
        self.constraints[i].0
    }

    fn constraint(&mut self, i: usize, x: &[f64]) -> Result<f64, Self::Error> {
        Ok((self.constraints[i].1)(x))
    }
}

fn rosenbrock(x: &[f64]) -> f64 {
    100.0 * (x[1] - x[0] * x[0]).powi(2) + (1.0 - x[0]).powi(2)
}

fn hs35(x: &[f64]) -> f64 {
    9.0 - 8.0 * x[0] - 6.0 * x[1] - 4.0 * x[2]
        + 2.0 * x[0] * x[0]
        + 2.0 * x[1] * x[1]
        + x[2] * x[2]
        + 2.0 * x[0] * x[1]
        + 2.0 * x[0] * x[2]
}

fn hs71(x: &[f64]) -> f64 {
    x[0] * x[3] * (x[0] + x[1] + x[2]) + x[2]
}

fn tutorial(x: &[f64]) -> f64 {
    (x[0] - 1.0).powi(2) + (x[1] - 2.5).powi(2)
}

fn weighted(x: &[f64]) -> f64 {
    x[0] * x[0] + 2.0 * x[1] * x[1] + 3.0 * x[2] * x[2]
}

/// The problem each case is named after, defined here rather than carried in
/// the JSON, in the constraint order of the generator.
fn problem(name: &str) -> Constrained {
    use ConstraintKind::{Eq, Ineq};
    let (f, constraints): (Function, Vec<(ConstraintKind, Function)>) = match name {
        "hs35" => (hs35, vec![(Ineq, |x| 3.0 - x[0] - x[1] - 2.0 * x[2])]),
        "hs71" => (
            hs71,
            vec![
                (Ineq, |x| x[0] * x[1] * x[2] * x[3] - 25.0),
                (Eq, |x| x.iter().map(|v| v * v).sum::<f64>() - 40.0),
            ],
        ),
        "tutorial" => (
            tutorial,
            vec![
                (Ineq, |x| x[0] - 2.0 * x[1] + 2.0),
                (Ineq, |x| -x[0] - 2.0 * x[1] + 6.0),
                (Ineq, |x| -x[0] + 2.0 * x[1] + 2.0),
            ],
        ),
        "rosenbrock_disc" => (
            rosenbrock,
            vec![(Ineq, |x| 1.5 - x[0] * x[0] - x[1] * x[1])],
        ),
        "weighted_plane" => (weighted, vec![(Eq, |x| x[0] + x[1] + x[2] - 1.0)]),
        other => panic!("fixture case {other} has no problem"),
    };
    Constrained { f, constraints }
}

fn central_gradient(function: Function, x: &[f64]) -> Vec<f64> {
    let h = 1e-6;
    (0..x.len())
        .map(|i| {
            let (mut plus, mut minus) = (x.to_vec(), x.to_vec());
            plus[i] += h;
            minus[i] -= h;
            (function(&plus) - function(&minus)) / (2.0 * h)
        })
        .collect()
}

fn max_violation(problem: &Constrained, x: &[f64]) -> f64 {
    problem
        .constraints
        .iter()
        .map(|(kind, c)| match kind {
            ConstraintKind::Eq => c(x).abs(),
            ConstraintKind::Ineq => (-c(x)).max(0.0),
        })
        .fold(0.0, f64::max)
}

/// The largest component of `g - A' lambda`, projected onto the bound cone at
/// `x`: the same measure the generator records for SciPy.
fn kkt_residual(problem: &Constrained, bounds: &Bounds, x: &[f64], multipliers: &[f64]) -> f64 {
    let mut residual = central_gradient(problem.f, x);
    for ((_, c), lambda) in problem.constraints.iter().zip(multipliers) {
        for (r, a) in residual.iter_mut().zip(central_gradient(*c, x)) {
            *r -= lambda * a;
        }
    }
    residual
        .iter()
        .enumerate()
        .map(|(i, &r)| {
            let r = if (x[i] - bounds.lower[i]).abs() <= 1e-10 {
                r.min(0.0)
            } else {
                r
            };
            let r = if (x[i] - bounds.upper[i]).abs() <= 1e-10 {
                r.max(0.0)
            } else {
                r
            };
            r.abs()
        })
        .fold(0.0, f64::max)
}

fn load() -> Fixture {
    serde_json::from_str(include_str!("fixtures/slsqp.json"))
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
fn every_case_meets_the_optimality_scipy_met() {
    let fixture = load();
    for (name, case) in &fixture.cases {
        let mut objective = problem(name);
        let n = case.x0.len();
        let bounds = case.bounds.as_ref().map(|pairs| Bounds {
            lower: pairs
                .iter()
                .map(|p| p[0].unwrap_or(f64::NEG_INFINITY))
                .collect(),
            upper: pairs
                .iter()
                .map(|p| p[1].unwrap_or(f64::INFINITY))
                .collect(),
        });
        let open = Bounds {
            lower: vec![f64::NEG_INFINITY; n],
            upper: vec![f64::INFINITY; n],
        };
        let cone = bounds.clone().unwrap_or(open);
        let problem = Problem {
            x0: case.x0.clone(),
            bounds,
        };
        let method = Method::Slsqp(SlsqpOptions {
            ftol: Some(case.ftol),
        });
        let common = Common {
            maxiter: Some(200),
            ..Common::default()
        };
        let result = minimize(&mut objective, &problem, &method, &common)
            .expect("an infallible objective cannot fail");
        assert_eq!(
            result.status,
            Termination::Converged(Converged::FTol),
            "{name}: {result:?}"
        );
        assert!(
            (result.fun - case.fun).abs() <= VALUE_TOLERANCE,
            "{name}: fun {} against {}",
            result.fun,
            case.fun
        );
        let violation = max_violation(&objective, &result.x);
        assert!(
            violation <= case.max_violation.max(VIOLATION_TOLERANCE),
            "{name}: violation {violation} against {}",
            case.max_violation
        );
        let multipliers = result
            .multipliers
            .as_deref()
            .expect("SLSQP reports multipliers");
        let residual = kkt_residual(&objective, &cone, &result.x, multipliers);
        assert!(
            residual <= case.kkt_residual + KKT_TOLERANCE,
            "{name}: KKT residual {residual} against {}",
            case.kkt_residual
        );
    }
}
