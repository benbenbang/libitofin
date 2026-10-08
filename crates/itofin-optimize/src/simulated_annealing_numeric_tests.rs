use super::*;
use serde_json::Value;
use std::convert::Infallible;

#[test]
fn metropolis_probabilities_match_independent_decimal_acceptance_reference() {
    let mut count = 0;
    for line in include_str!("../tests/fixtures/hybrid_simulated_annealing.jsonl").lines() {
        let record: Value = serde_json::from_str(line).unwrap();
        if record["kind"] != "acceptance" {
            continue;
        }
        let value = &record["value"];
        let current = value["current"].as_f64().unwrap();
        let trial = value["trial"].as_f64().unwrap();
        let temperature = value["temperature"].as_f64().unwrap();
        let actual = if trial <= current {
            1.0
        } else {
            uphill_probability(current, trial, temperature)
        };
        let expected = value["probability"].as_f64().unwrap();
        assert!((actual - expected).abs() <= 1e-15, "{actual} != {expected}");
        count += 1;
    }
    assert_eq!(count, 6);
}

#[test]
fn huge_temperature_retains_nonzero_acceptance_when_raw_delta_overflows() {
    let probability = uphill_probability(-1e308, 1e308, 1e308);
    assert!((probability - (-2.0f64).exp()).abs() <= 1e-15);
    assert!(probability > 0.13 && probability < 0.14);
    assert_eq!(uphill_probability(-1e308, 1e308, 1.0), 0.0);
    assert_eq!(
        uphill_probability(-f64::MAX, f64::MAX, f64::MAX),
        (-2.0f64).exp()
    );
}

#[test]
fn zero_normalized_direction_preserves_exact_physical_bits() {
    let bounds = Bounds {
        lower: vec![-1e308],
        upper: vec![7e307],
    };
    let space = BoxSpace::new(&bounds);
    for point in [-0.0, 0.0, 1e-300, -1e-300, 7e307, -1e308] {
        for reflect in [true, false] {
            assert_eq!(
                space.moved(point, 0, 0.0, reflect).to_bits(),
                point.to_bits()
            );
            assert_eq!(
                space.moved(point, 0, -0.0, reflect).to_bits(),
                point.to_bits()
            );
        }
    }
}

#[test]
fn reflection_maps_bound_crossings_without_clipping_atoms() {
    let bounds = Bounds {
        lower: vec![-2.0],
        upper: vec![2.0],
    };
    let space = BoxSpace::new(&bounds);
    assert_eq!(space.moved(-2.0, 0, -0.25, true), -1.0);
    assert_eq!(space.moved(2.0, 0, 0.25, true), 1.0);
    assert_eq!(space.moved(-2.0, 0, -1.0, true), 2.0);
    assert_eq!(space.moved(2.0, 0, 1.0, true), -2.0);
    assert_eq!(space.moved(-2.0, 0, -0.25, false), -2.0);
}

#[test]
fn repeated_seed_is_exact_and_fixed_coordinates_never_move() {
    let p = Problem {
        x0: vec![0.0, 7.0],
        bounds: Some(Bounds {
            lower: vec![-2.0, 7.0],
            upper: vec![2.0, 7.0],
        }),
    };
    let common = Common {
        maxiter: Some(12),
        ..Common::default()
    };
    let mut first = Recorder::default();
    let mut second = Recorder::default();
    let a = solve(&mut first, &p, options(), common.clone());
    let b = solve(&mut second, &p, options(), common);
    assert_eq!(a, b);
    assert_eq!(first.points, second.points);
    assert_eq!(first.callbacks, second.callbacks);
    assert!(first.points.iter().all(|point| point[1] == 7.0));
    let space = BoxSpace::new(p.bounds.as_ref().unwrap());
    let mut random = Random::new(0);
    space.proposal(&p.x0, 0.25, &mut random);
    let mut expected = Random::new(0);
    expected.unit();
    assert_eq!(random.unit().to_bits(), expected.unit().to_bits());
}

#[test]
fn neighborhood_objective_spread_must_also_meet_fatol() {
    let p = Problem {
        x0: vec![2.0],
        ..problem()
    };
    for (fatol, expected) in [
        (0.0, Termination::MaxIterations),
        (100.0, Termination::Converged(Converged::XTol)),
    ] {
        let mut objective =
            |point: &[f64]| -> Result<f64, Infallible> { Ok((point[0] - 3.0).powi(2)) };
        let result = crate::minimize(
            &mut objective,
            &p,
            &Method::HybridSimulatedAnnealing(HybridSimulatedAnnealingOptions {
                xatol: Some(1.0),
                fatol: Some(fatol),
                local_search_steps: 1,
                ..options()
            }),
            &Common {
                maxiter: Some(2),
                ..Common::default()
            },
        )
        .unwrap();
        assert_eq!(result.status, expected);
    }
}

#[test]
fn collapsed_physical_proposals_and_polls_do_not_charge_evaluations() {
    let p = Problem {
        x0: vec![1.0],
        bounds: Some(Bounds {
            lower: vec![1.0],
            upper: vec![f64::from_bits(1.0f64.to_bits() + 1)],
        }),
    };
    let mut objective = Recorder {
        constant: true,
        ..Recorder::default()
    };
    let result = solve(
        &mut objective,
        &p,
        HybridSimulatedAnnealingOptions {
            step_size: 1e-300,
            ..options()
        },
        Common {
            maxiter: Some(3),
            ..Common::default()
        },
    );
    assert_eq!((result.nit, result.nfev), (3, 1));
    assert_eq!(result.status, Termination::MaxIterations);
    assert_eq!(objective.points, vec![vec![1.0]]);
    assert!(objective.callbacks.iter().all(|row| row.3 == 1));
}

#[test]
fn finite_neighborhood_spread_overflow_does_not_become_nonfinite_objective() {
    let mut objective = |point: &[f64]| -> Result<f64, Infallible> {
        Ok(if point[0] >= 0.0 { f64::MAX } else { -f64::MAX })
    };
    let result = crate::minimize(
        &mut objective,
        &problem(),
        &Method::HybridSimulatedAnnealing(HybridSimulatedAnnealingOptions {
            xatol: Some(1.0),
            fatol: Some(f64::MAX),
            ..options()
        }),
        &Common {
            maxiter: Some(2),
            ..Common::default()
        },
    )
    .unwrap();
    assert_ne!(result.status, Termination::Nonfinite);
    assert_eq!(result.fun, -f64::MAX);
}

#[test]
fn common_tolerance_is_fallback_and_explicit_zero_is_preserved() {
    for (explicit, status) in [
        (None, Termination::Converged(Converged::XTol)),
        (Some(0.0), Termination::MaxIterations),
    ] {
        let mut objective = Recorder {
            constant: true,
            ..Recorder::default()
        };
        let result = solve(
            &mut objective,
            &problem(),
            HybridSimulatedAnnealingOptions {
                xatol: explicit,
                fatol: explicit,
                ..options()
            },
            Common {
                tol: Some(1.0),
                maxiter: Some(2),
                ..Common::default()
            },
        );
        assert_eq!(result.status, status);
    }
}
