use super::*;
use std::convert::Infallible;

fn particle(x: f64, velocity: f64, best: f64) -> Particle {
    Particle {
        x: vec![x],
        f: 0.0,
        velocity: vec![velocity],
        best: vec![best],
        best_value: 0.0,
    }
}

#[test]
fn velocity_formula_uses_frozen_physical_differences_and_rng_order() {
    let p = problem();
    let space = BoxSpace::new(p.bounds.as_ref().unwrap());
    let old = particle(1.0, 0.1, -1.0);
    let opts = ParticleSwarmOptions {
        velocity_clamp: 1.0,
        ..ParticleSwarmOptions::default()
    };
    let mut oracle = Random::new(0);
    let r1 = oracle.unit();
    let r2 = oracle.unit();
    let expected = 0.7 * 0.1 + 1.4 * r1 * (-2.0 / 4.0) + 1.4 * r2 * (-0.5 / 4.0);
    let (x, v) = moved(&old, &[0.5], &space, &opts, &mut Random::new(0));
    assert_eq!(v, vec![expected]);
    assert_eq!(x[0], space.coordinate(0, 0.75 + expected));
    assert_eq!(old.x, vec![1.0]);
}

#[test]
fn coefficients_zero_still_consume_two_draws_and_preserve_physical_bits() {
    let bounds = Bounds {
        lower: vec![0.0, 7.0],
        upper: vec![1.0, 7.0],
    };
    let space = BoxSpace::new(&bounds);
    let old = Particle {
        x: vec![f64::from_bits(1), 7.0],
        f: 0.0,
        velocity: vec![0.0],
        best: vec![0.5, 7.0],
        best_value: 0.0,
    };
    let opts = ParticleSwarmOptions {
        inertia: 0.0,
        cognitive: 0.0,
        social: 0.0,
        ..ParticleSwarmOptions::default()
    };
    let mut random = Random::new(4);
    let (x, v) = moved(&old, &[1.0, 7.0], &space, &opts, &mut random);
    assert_eq!(x[0].to_bits(), old.x[0].to_bits());
    assert_eq!(x[1], 7.0);
    assert_eq!(v, vec![0.0]);
    let mut oracle = Random::new(4);
    oracle.unit();
    oracle.unit();
    assert_eq!(random.unit(), oracle.unit());
}

#[test]
fn velocity_clipping_and_absorbing_boundaries_are_symmetric() {
    let bounds = Bounds {
        lower: vec![0.0],
        upper: vec![1.0],
    };
    let space = BoxSpace::new(&bounds);
    let opts = ParticleSwarmOptions {
        inertia: 1.0,
        cognitive: 0.0,
        social: 0.0,
        ..ParticleSwarmOptions::default()
    };
    for (x, v, expected_x, expected_v) in [
        (0.5, 0.8, 0.7, 0.2),
        (0.5, -0.8, 0.3, -0.2),
        (0.9, 0.8, 1.0, 0.0),
        (0.1, -0.8, 0.0, 0.0),
        (1.0, 0.2, 1.0, 0.0),
        (0.0, -0.2, 0.0, 0.0),
    ] {
        let (point, velocity) = moved(&particle(x, v, x), &[x], &space, &opts, &mut Random::new(0));
        assert!((point[0] - expected_x).abs() < 1e-15);
        assert_eq!(velocity[0], expected_v);
    }
}

#[test]
fn convergence_requires_current_physical_spread_values_and_velocity() {
    let bounds = Bounds {
        lower: vec![0.0],
        upper: vec![1.0],
    };
    let space = BoxSpace::new(&bounds);
    let mut population = vec![particle(0.5, 0.0, 0.5); 4];
    assert!(converged(&population, &space, 0.0, 0.0));
    population[0].velocity[0] = f64::from_bits(1);
    assert!(!converged(&population, &space, 0.0, 0.0));
    population[0].velocity[0] = 0.0;
    population[0].x[0] = f64::from_bits(0.5f64.to_bits() + 1);
    assert!(!converged(&population, &space, 0.0, 0.0));
    assert!(converged(&population, &space, 1e-6, 0.0));
    population[0].f = 1.0;
    assert!(!converged(&population, &space, 1e-6, 0.5));
}

#[test]
fn zero_tolerance_detects_subnormal_physical_spread_even_when_ratio_underflows() {
    let bounds = Bounds {
        lower: vec![0.0],
        upper: vec![1e308],
    };
    let space = BoxSpace::new(&bounds);
    let population = vec![
        particle(0.0, 0.0, 0.0),
        particle(f64::from_bits(1), 0.0, 0.0),
    ];
    assert_eq!(
        (population[1].x[0] - population[0].x[0]) / space.width(0),
        0.0
    );
    assert!(!converged(&population, &space, 0.0, 0.0));
}

#[test]
fn physical_best_differences_do_not_disappear_in_normalized_subtraction() {
    let lower = -1e16;
    let upper = 1e16;
    let bounds = Bounds {
        lower: vec![lower],
        upper: vec![upper],
    };
    let space = BoxSpace::new(&bounds);
    let old = particle(1.0, 0.0, 1.0);
    assert_eq!(space.fraction(0, 0.0), space.fraction(0, 1.0));
    let (_, v) = moved(
        &old,
        &[0.0],
        &space,
        &ParticleSwarmOptions::default(),
        &mut Random::new(0),
    );
    assert!(v[0] < 0.0);
}

#[test]
fn huge_and_subnormal_boxes_never_evaluate_infeasible_or_nonfinite_points() {
    for (lower, upper, x0) in [
        (1e308, 1.7e308, 1.2e308),
        (-1e308, -1e307, -3e307),
        (-8e307, 8e307, 0.0),
        (0.0, f64::from_bits(10), f64::from_bits(4)),
    ] {
        let p = Problem {
            x0: vec![x0],
            bounds: Some(Bounds {
                lower: vec![lower],
                upper: vec![upper],
            }),
        };
        let opts = ParticleSwarmOptions {
            global: GlobalOptions {
                seed: 13,
                population_size: Some(8),
                xatol: Some(0.0),
                fatol: Some(0.0),
                ..GlobalOptions::default()
            },
            velocity_clamp: 1.0,
            ..ParticleSwarmOptions::default()
        };
        let mut calls = 0;
        let result = crate::minimize(
            &mut |x: &[f64]| -> Result<f64, Infallible> {
                assert!(x[0].is_finite() && x[0] >= lower && x[0] <= upper);
                calls += 1;
                Ok(((x[0] - lower) / (upper - lower) - 0.25).powi(2) - 3.0)
            },
            &p,
            &Method::ParticleSwarm(opts),
            &Common {
                maxiter: Some(20),
                ..Common::default()
            },
        )
        .unwrap();
        assert_eq!(result.nfev, calls);
        assert_eq!(result.njev, 0);
        assert!(result.fun.is_finite());
    }
}

#[test]
fn signed_quadratic_finds_negative_minimum_and_preserves_fixed_coordinate() {
    let p = Problem {
        x0: vec![3.0, -2.0, 7.0],
        bounds: Some(Bounds {
            lower: vec![-4.0, -3.0, 7.0],
            upper: vec![4.0, 3.0, 7.0],
        }),
    };
    let opts = ParticleSwarmOptions {
        global: GlobalOptions {
            seed: 42,
            population_size: Some(32),
            ..GlobalOptions::default()
        },
        ..ParticleSwarmOptions::default()
    };
    let result = crate::minimize(
        &mut |x: &[f64]| -> Result<f64, Infallible> {
            assert_eq!(x[2], 7.0);
            Ok((x[0] - 0.75).powi(2) + 2.0 * (x[1] + 0.5).powi(2) - 3.0)
        },
        &p,
        &Method::ParticleSwarm(opts),
        &Common::default(),
    )
    .unwrap();
    assert!(result.success, "{result:?}");
    assert!((result.fun + 3.0).abs() < 1e-10);
    assert!((result.x[0] - 0.75).abs() < 1e-5);
    assert!((result.x[1] + 0.5).abs() < 1e-5);
    assert_eq!(result.njev, 0);
}

#[test]
fn seeded_swarm_finds_multimodal_himmelblau_quality_without_global_claim() {
    let p = Problem {
        x0: vec![0.0, 0.0],
        bounds: Some(Bounds {
            lower: vec![-6.0, -6.0],
            upper: vec![6.0, 6.0],
        }),
    };
    let opts = ParticleSwarmOptions {
        global: GlobalOptions {
            seed: 9,
            population_size: Some(64),
            ..GlobalOptions::default()
        },
        ..ParticleSwarmOptions::default()
    };
    let result = crate::minimize(
        &mut |x: &[f64]| -> Result<f64, Infallible> {
            Ok((x[0] * x[0] + x[1] - 11.0).powi(2) + (x[0] + x[1] * x[1] - 7.0).powi(2))
        },
        &p,
        &Method::ParticleSwarm(opts),
        &Common {
            maxiter: Some(1000),
            ..Common::default()
        },
    )
    .unwrap();
    assert!(result.fun < 1e-8, "{result:?}");
    assert!(result.x.iter().all(|x| x.abs() <= 6.0));
    assert_eq!(result.njev, 0);
}

#[test]
fn huge_box_trace_uses_physical_attraction_when_normalized_points_coincide() {
    let p = Problem {
        x0: vec![1.0],
        bounds: Some(Bounds {
            lower: vec![-1e20],
            upper: vec![1e20],
        }),
    };
    let opts = ParticleSwarmOptions {
        global: GlobalOptions {
            seed: 0,
            initial_population: Some(vec![vec![1.0], vec![4.0], vec![5.0], vec![6.0]]),
            xatol: Some(0.0),
            fatol: Some(0.0),
            ..GlobalOptions::default()
        },
        inertia: 0.0,
        cognitive: 0.0,
        social: 1.0,
        velocity_clamp: 0.2,
    };
    let mut points = Vec::new();
    let result = crate::minimize(
        &mut |x: &[f64]| -> Result<f64, Infallible> {
            points.push(x.to_vec());
            Ok(-7.0)
        },
        &p,
        &Method::ParticleSwarm(opts),
        &Common {
            maxiter: Some(1),
            ..Common::default()
        },
    )
    .unwrap();
    assert_eq!(
        points,
        vec![
            vec![1.0],
            vec![4.0],
            vec![5.0],
            vec![6.0],
            vec![1.0],
            vec![0.0],
            vec![0.0],
            vec![0.0]
        ]
    );
    assert_eq!(result.x, vec![1.0]);
    assert_eq!(result.fun, -7.0);
    assert_eq!((result.nit, result.nfev, result.njev), (1, 8, 0));
    assert_eq!(result.status, Termination::MaxIterations);
    assert!(!result.success);
}
