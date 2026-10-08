use std::convert::Infallible;

use itofin_optimize::{Bounds, Common, HybridSimulatedAnnealingOptions, Method, Problem, minimize};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut calls = 0;
    let mut objective = |x: &[f64]| -> Result<f64, Infallible> {
        calls += 1;
        Ok((x[0] - 1.25).powi(2) + 4.0 * (x[1] + 0.75).powi(2) - 3.0)
    };
    let problem = Problem {
        x0: vec![-3.0, 3.0],
        bounds: Some(Bounds {
            lower: vec![-4.0, -4.0],
            upper: vec![4.0, 4.0],
        }),
    };
    let method = Method::HybridSimulatedAnnealing(HybridSimulatedAnnealingOptions {
        seed: 42,
        xatol: Some(1e-7),
        fatol: Some(1e-10),
        ..HybridSimulatedAnnealingOptions::default()
    });
    let result = minimize(&mut objective, &problem, &method, &Common::default())?;
    assert!(result.success, "{}", result.message);
    assert!((result.x[0] - 1.25).abs() < 1e-5);
    assert!((result.x[1] + 0.75).abs() < 1e-5);
    assert!((result.fun + 3.0).abs() < 1e-8);
    assert_eq!(result.nfev, calls);
    assert_eq!(result.njev, 0);
    println!("x={:?}, scalar minimum={:.10}", result.x, result.fun);
    println!("cycles={}, objective calls={}", result.nit, result.nfev);
    println!("{}", result.status);
    Ok(())
}
