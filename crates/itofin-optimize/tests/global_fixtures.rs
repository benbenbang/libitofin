use itofin_optimize::{
    Bounds, Common, DifferentialEvolutionOptions, GlobalOptions, Method, Problem, minimize,
};
use serde::Deserialize;
use std::convert::Infallible;

#[derive(Deserialize)]
struct Fixture {
    provenance: Provenance,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Provenance {
    scipy_version: String,
    primary_reference: String,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    x0: Vec<f64>,
    bounds: Vec<[f64; 2]>,
    analytic_x: Vec<f64>,
    analytic_fun: f64,
    point_tolerance: f64,
    value_tolerance: f64,
    scipy_fun: f64,
}

fn value(name: &str, x: &[f64]) -> f64 {
    match name {
        "negative_quadratic" => (x[0] - 1.25).powi(2) + 4.0 * (x[1] + 0.75).powi(2) - 3.0,
        "multimodal_polynomial" => (x[0] - 1.0).powi(2) * ((x[0] + 2.0).powi(2) + 0.1),
        "shifted_rastrigin" => {
            15.0 + x
                .iter()
                .map(|v| v * v - 10.0 * (2.0 * std::f64::consts::PI * v).cos())
                .sum::<f64>()
        }
        "boundary_quadratic" => (x[0] - 3.0).powi(2) + (x[1] + 4.0).powi(2) - 20.0,
        "fixed_quadratic" => (x[0] - 2.0).powi(2) + (x[1] - 0.375).powi(2) - 7.0,
        _ => panic!("unknown fixture"),
    }
}

#[test]
fn differential_evolution_matches_analytic_and_independent_scipy_quality() {
    let fixture: Fixture = serde_json::from_str(include_str!("fixtures/global.json")).unwrap();
    assert!(!fixture.provenance.scipy_version.is_empty());
    assert_eq!(
        fixture.provenance.primary_reference,
        "https://doi.org/10.1023/A:1008202821328"
    );
    for case in fixture.cases {
        let problem = Problem {
            x0: case.x0.clone(),
            bounds: Some(Bounds {
                lower: case.bounds.iter().map(|b| b[0]).collect(),
                upper: case.bounds.iter().map(|b| b[1]).collect(),
            }),
        };
        let method = Method::DifferentialEvolution(DifferentialEvolutionOptions {
            global: GlobalOptions {
                seed: 42,
                population_size: Some(64),
                ..Default::default()
            },
            ..Default::default()
        });
        let mut calls = 0;
        let mut objective = |x: &[f64]| -> Result<f64, Infallible> {
            calls += 1;
            for (v, b) in x.iter().zip(&case.bounds) {
                assert!(*v >= b[0] && *v <= b[1]);
            }
            Ok(value(&case.name, x))
        };
        let result = minimize(&mut objective, &problem, &method, &Common::default()).unwrap();
        assert!(result.success, "{}: {:?}", case.name, result);
        assert_eq!(result.nfev, calls);
        assert_eq!(result.njev, 0);
        assert!(
            (result.fun - case.analytic_fun).abs() <= case.value_tolerance,
            "{}: {}",
            case.name,
            result.fun
        );
        assert!(
            (result.fun - case.scipy_fun).abs() <= case.value_tolerance,
            "{}",
            case.name
        );
        for (actual, expected) in result.x.iter().zip(&case.analytic_x) {
            assert!(
                (actual - expected).abs() <= case.point_tolerance,
                "{}: {:?}",
                case.name,
                result.x
            );
        }
        assert_eq!(result.fun.to_bits(), value(&case.name, &result.x).to_bits());
    }
}
