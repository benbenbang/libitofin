use super::*;
use std::convert::Infallible;

#[test]
fn normalized_attraction_and_noise_match_manual_equation_with_fixed_draw_order() {
    let bounds = Bounds {
        lower: vec![0.0, -20.0, 7.0],
        upper: vec![2.0, 20.0, 7.0],
    };
    let space = BoxSpace::new(&bounds);
    let x = vec![1.0, 0.0, 7.0];
    let target = vec![0.0, 10.0, 7.0];
    let opts = FireflyOptions::default();
    let beta = (-0.3125f64).exp();
    let mut oracle = Random::new(9);
    let expected = vec![
        1.0 - beta + 0.25 * (oracle.unit() - 0.5) * 2.0,
        beta * 10.0 + 0.25 * (oracle.unit() - 0.5) * 40.0,
        7.0,
    ];
    let actual = moved(&x, Some(&target), &space, &opts, 0.25, &mut Random::new(9));
    for (actual, expected) in actual.iter().zip(expected) {
        assert!((actual - expected).abs() <= 4e-15);
    }
}

#[test]
fn zero_alpha_draws_uniforms_and_preserves_no_move_physical_bits() {
    let bounds = Bounds {
        lower: vec![0.0, 7.0],
        upper: vec![1.0, 7.0],
    };
    let space = BoxSpace::new(&bounds);
    let x = vec![f64::from_bits(1), 7.0];
    let mut random = Random::new(19);
    let actual = moved(
        &x,
        None,
        &space,
        &FireflyOptions::default(),
        0.0,
        &mut random,
    );
    assert_eq!(
        actual.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
        x.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
    );
    let mut oracle = Random::new(19);
    oracle.unit();
    assert_eq!(random.unit(), oracle.unit());
}

#[test]
fn physical_attraction_survives_identical_normalized_offsets() {
    let bounds = Bounds {
        lower: vec![-1e20],
        upper: vec![1e20],
    };
    let space = BoxSpace::new(&bounds);
    assert_eq!(space.fraction(0, 1.0), space.fraction(0, 4.0));
    let opts = FireflyOptions {
        alpha: 0.0,
        beta0: 0.5,
        gamma: 0.0,
        ..FireflyOptions::default()
    };
    assert_eq!(
        moved(
            &[4.0],
            Some(&[1.0]),
            &space,
            &opts,
            0.0,
            &mut Random::new(0)
        ),
        vec![2.5]
    );
}

#[test]
fn boundary_reflection_is_not_absorption() {
    let bounds = Bounds {
        lower: vec![0.0],
        upper: vec![1.0],
    };
    let space = BoxSpace::new(&bounds);
    let mut oracle = Random::new(0);
    let noise = oracle.unit() - 0.5;
    assert!(noise > 0.0);
    let actual = moved(
        &[1.0],
        None,
        &space,
        &FireflyOptions::default(),
        1.0,
        &mut Random::new(0),
    );
    assert!((actual[0] - (1.0 - noise)).abs() < 1e-15);
    assert!(actual[0] < 1.0);
    let actual = moved(
        &[0.0],
        None,
        &space,
        &FireflyOptions::default(),
        1.0,
        &mut Random::new(19),
    );
    let mut oracle = Random::new(19);
    let expected = (oracle.unit() - 0.5).abs();
    assert!((actual[0] - expected).abs() < 1e-15);
}

#[test]
fn spread_convergence_requires_decayed_noise_and_exact_zero_physical_spread() {
    let bounds = Bounds {
        lower: vec![0.0],
        upper: vec![1e308],
    };
    let space = BoxSpace::new(&bounds);
    let mut population = vec![
        Firefly {
            x: vec![0.0],
            f: 0.0
        };
        4
    ];
    assert!(!converged(&population, &space, 0.25, 1e-6, 0.0));
    assert!(converged(&population, &space, 0.0, 0.0, 0.0));
    population[1].x[0] = f64::from_bits(1);
    assert_eq!(population[1].x[0] / space.width(0), 0.0);
    assert!(!converged(&population, &space, 0.0, 0.0, 0.0));
    assert!(converged(&population, &space, 0.0, 1e-6, 0.0));
    population[0].f = f64::MAX;
    population[1].f = -f64::MAX;
    assert!(!converged(&population, &space, 0.0, 1e-6, f64::MAX));
}

#[test]
fn geometric_noise_decay_occurs_only_after_completed_generation() {
    let mut opts = options();
    opts.alpha = 0.4;
    opts.alpha_decay = 0.5;
    let mut objective = Recorder {
        constant: true,
        ..Recorder::default()
    };
    let result = solve(
        &mut objective,
        &problem(),
        opts.clone(),
        Common {
            maxiter: Some(2),
            ..Common::default()
        },
    );
    let p = problem();
    let space = BoxSpace::new(p.bounds.as_ref().unwrap());
    let mut oracle = Random::new(0);
    let first: Vec<Vec<f64>> = opts
        .global
        .initial_population
        .as_ref()
        .unwrap()
        .iter()
        .map(|x| moved(x, None, &space, &opts, 0.4, &mut oracle))
        .collect();
    let second: Vec<Vec<f64>> = first
        .iter()
        .map(|x| moved(x, None, &space, &opts, 0.2, &mut oracle))
        .collect();
    assert_eq!(&objective.points[4..8], first);
    assert_eq!(&objective.points[8..12], second);
    assert_eq!((result.nit, result.nfev), (2, 12));
}

#[test]
fn extreme_finite_and_subnormal_boxes_remain_feasible() {
    for (lo, hi, x0) in [
        (1e308, 1.7e308, 1.2e308),
        (-1e308, -1e307, -3e307),
        (-8e307, 8e307, 0.0),
        (0.0, f64::from_bits(10), f64::from_bits(4)),
    ] {
        let p = Problem {
            x0: vec![x0, 7.0],
            bounds: Some(Bounds {
                lower: vec![lo, 7.0],
                upper: vec![hi, 7.0],
            }),
        };
        let opts = FireflyOptions {
            global: GlobalOptions {
                seed: 13,
                population_size: Some(8),
                xatol: Some(0.0),
                fatol: Some(0.0),
                ..GlobalOptions::default()
            },
            alpha: 1.0,
            ..FireflyOptions::default()
        };
        let mut calls = 0;
        let result = crate::minimize(
            &mut |x: &[f64]| -> Result<f64, Infallible> {
                assert!(x[0].is_finite() && x[0] >= lo && x[0] <= hi);
                assert_eq!(x[1], 7.0);
                calls += 1;
                Ok(((x[0] - lo) / (hi - lo) - 0.25).powi(2) - 3.0)
            },
            &p,
            &Method::Firefly(opts),
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
fn seeded_signed_quadratic_quality_and_reproducibility() {
    let p = Problem {
        x0: vec![3.0, -2.0, 7.0],
        bounds: Some(Bounds {
            lower: vec![-4.0, -3.0, 7.0],
            upper: vec![4.0, 3.0, 7.0],
        }),
    };
    let opts = FireflyOptions {
        global: GlobalOptions {
            seed: 42,
            population_size: Some(32),
            ..GlobalOptions::default()
        },
        ..FireflyOptions::default()
    };
    let run = || {
        crate::minimize(
            &mut |x: &[f64]| -> Result<f64, Infallible> {
                assert_eq!(x[2], 7.0);
                Ok((x[0] - 0.75).powi(2) + 2.0 * (x[1] + 0.5).powi(2) - 3.0)
            },
            &p,
            &Method::Firefly(opts.clone()),
            &Common::default(),
        )
        .unwrap()
    };
    let first = run();
    let second = run();
    assert_eq!(first, second);
    assert!(first.success, "{first:?}");
    assert!((first.fun + 3.0).abs() < 1e-10, "{first:?}");
    assert!((first.x[0] - 0.75).abs() < 1e-5);
    assert!((first.x[1] + 0.5).abs() < 1e-5);
    assert_eq!(first.njev, 0);
}

#[test]
fn seeded_himmelblau_multimodal_quality_without_global_guarantee() {
    let p = Problem {
        x0: vec![0.0, 0.0],
        bounds: Some(Bounds {
            lower: vec![-6.0, -6.0],
            upper: vec![6.0, 6.0],
        }),
    };
    let opts = FireflyOptions {
        global: GlobalOptions {
            seed: 9,
            population_size: Some(32),
            ..GlobalOptions::default()
        },
        ..FireflyOptions::default()
    };
    let result = crate::minimize(
        &mut |x: &[f64]| -> Result<f64, Infallible> {
            Ok((x[0] * x[0] + x[1] - 11.0).powi(2) + (x[0] + x[1] * x[1] - 7.0).powi(2))
        },
        &p,
        &Method::Firefly(opts),
        &Common::default(),
    )
    .unwrap();
    assert!(result.fun < 1e-8, "{result:?}");
    assert!(result.x.iter().all(|x| x.abs() <= 6.0));
    assert_eq!(result.njev, 0);
}

#[test]
fn convergence_respects_common_tolerance_and_quiet_population() {
    let mut opts = options();
    opts.global.initial_population = Some(vec![vec![0.25]; 4]);
    opts.global.xatol = None;
    opts.global.fatol = None;
    opts.alpha = 0.1;
    let mut objective = Recorder::default();
    let result = solve(
        &mut objective,
        &problem(),
        opts,
        Common {
            tol: Some(0.2),
            ..Common::default()
        },
    );
    assert!(result.success);
    assert_eq!((result.nit, result.nfev), (0, 4));
    assert!(objective.callbacks.is_empty());
    let mut opts = options();
    opts.global.initial_population = Some(vec![vec![0.25]; 4]);
    opts.alpha = 0.0;
    let mut objective = Recorder::default();
    let result = solve(&mut objective, &problem(), opts, Common::default());
    assert!(result.success);
    assert_eq!(result.status, Termination::Converged(Converged::XTol));
}
