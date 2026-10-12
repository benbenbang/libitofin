use libitofin::{math::array::Array, processes::G2Process, stochasticprocess::StochasticProcess};

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 8e-14 * expected.abs().max(1e-300),
        "{actual:e} != {expected:e}"
    );
}

fn zero() -> Array {
    Array::from([0.0, 0.0])
}

#[test]
fn public_parameters_dimensions_and_bare_state() {
    let p = G2Process::new(0.1, 0.01, 0.2, 0.008, -0.75).unwrap();
    assert_eq!(
        (p.a(), p.sigma(), p.b(), p.eta(), p.rho()),
        (0.1, 0.01, 0.2, 0.008, -0.75)
    );
    assert_eq!((p.x0(), p.y0()), (0.0, 0.0));
    let dynamic: &dyn StochasticProcess = &p;
    assert_eq!((dynamic.size(), dynamic.factors()), (2, 2));
    assert_eq!(dynamic.initial_values().unwrap(), zero());
    let x = Array::from([0.015, -0.007]);
    assert_eq!(
        p.drift(1.0, &x).unwrap(),
        Array::from([-0.1 * 0.015, -0.2 * -0.007])
    );
    let e = p.expectation(1.0, &x, 0.25).unwrap();
    close(e[0], 0.015 * (-0.025_f64).exp());
    close(e[1], -0.007 * (-0.05_f64).exp());
}

#[test]
fn instantaneous_covariance_is_correct_for_unequal_volatilities() {
    let p = G2Process::new(0.1, 0.03, 0.4, 0.007, -0.6).unwrap();
    let d = p.diffusion(0.0, &zero()).unwrap();
    close(d[(1, 0)], -0.6 * 0.007);
    let c = &d * &d.transpose();
    close(c[(0, 0)], 0.03_f64.powi(2));
    close(c[(1, 1)], 0.007_f64.powi(2));
    close(c[(0, 1)], -0.6 * 0.03 * 0.007);
    let dt = 1e-8;
    let step = p.covariance(0.0, &zero(), dt).unwrap();
    for i in 0..2 {
        for j in 0..2 {
            assert!((step[(i, j)] / dt / c[(i, j)] - 1.0).abs() < 5e-9);
        }
    }
}

#[test]
fn exact_transition_covariance_and_root_match_closed_form() {
    for (a, b, rho) in [
        (0.1_f64, 0.2_f64, -0.75),
        (0.3, 0.3, 1.0),
        (0.3, 0.7, -1.0),
        (-0.2, 0.2, 0.4),
    ] {
        let p = G2Process::new(a, 0.03, b, 0.007, rho).unwrap();
        let dt = 0.8;
        let integral = |k: f64| {
            if k == 0.0 {
                dt
            } else {
                -(-k * dt).exp_m1() / k
            }
        };
        let expected = [
            [
                0.03_f64.powi(2) * integral(2.0 * a),
                rho * 0.03 * 0.007 * integral(a + b),
            ],
            [
                rho * 0.03 * 0.007 * integral(a + b),
                0.007_f64.powi(2) * integral(2.0 * b),
            ],
        ];
        let cov = p.covariance(3.0, &zero(), dt).unwrap();
        let s = p.std_deviation(3.0, &zero(), dt).unwrap();
        let rebuilt = &s * &s.transpose();
        for i in 0..2 {
            for j in 0..2 {
                close(cov[(i, j)], expected[i][j]);
                close(rebuilt[(i, j)], expected[i][j]);
            }
        }
        let dw = Array::from([0.75, -0.4]);
        let next = p.evolve(3.0, &zero(), dt, &dw).unwrap();
        close(next[0], s[(0, 0)] * dw[0]);
        close(next[1], s[(1, 0)] * dw[0] + s[(1, 1)] * dw[1]);
    }
}

#[test]
fn zero_steps_and_deterministic_factors_do_not_divide_by_zero() {
    for (sigma, eta, rho) in [
        (0.0, 0.0, 1.0),
        (0.0, 0.007, -1.0),
        (0.03, 0.0, 1.0),
        (0.03, 0.007, 0.0),
    ] {
        let p = G2Process::new(0.0, sigma, 0.0, eta, rho).unwrap();
        let x = Array::from([0.1, -0.2]);
        assert_eq!(
            p.evolve(0.0, &x, 0.0, &Array::from([2.0, -3.0])).unwrap(),
            x
        );
        let c = p.covariance(0.0, &x, 0.7).unwrap();
        close(c[(0, 0)], sigma * sigma * 0.7);
        close(c[(1, 1)], eta * eta * 0.7);
        close(c[(0, 1)], rho * sigma * eta * 0.7);
    }
}

#[test]
fn tiny_reversion_uses_expm1_without_a_cutoff() {
    let p = G2Process::new(1e-16, 0.03, 2e-16, 0.007, 0.8).unwrap();
    let dt = 1e15;
    let c = p.covariance(0.0, &zero(), dt).unwrap();
    close(c[(0, 0)], 0.03_f64.powi(2) * (-(-0.2_f64).exp_m1()) / 2e-16);
    close(
        c[(1, 1)],
        0.007_f64.powi(2) * (-(-0.4_f64).exp_m1()) / 4e-16,
    );
    close(
        c[(0, 1)],
        0.8 * 0.03 * 0.007 * (-(-0.3_f64).exp_m1()) / 3e-16,
    );
}

#[test]
fn log_scaling_avoids_unnecessary_volatility_and_growth_overflow() {
    let p = G2Process::new(1e200, 1e200, 1e200, 1e200, 0.5).unwrap();
    let c = p.covariance(0.0, &zero(), 1.0).unwrap();
    close(c[(0, 0)], 5e199);
    close(c[(1, 1)], 5e199);
    close(c[(0, 1)], 2.5e199);
    let p = G2Process::new(-1.0, 1e-200, -1.0, 1e-200, 0.5).unwrap();
    let x = Array::from([1e-200, -1e-200]);
    let e = p.expectation(0.0, &x, 800.0).unwrap();
    close(e[0], (800.0 + 1e-200_f64.ln()).exp());
    close(e[1], -e[0]);
    assert!(p.covariance(0.0, &x, 800.0).unwrap()[(0, 0)].is_finite());
}

#[test]
fn constructor_rejects_invalid_parameters() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for i in 0..5 {
            let mut p = [0.1, 0.01, 0.2, 0.02, 0.5];
            p[i] = bad;
            assert!(G2Process::new(p[0], p[1], p[2], p[3], p[4]).is_err());
        }
    }
    assert!(G2Process::new(0.1, -0.01, 0.2, 0.02, 0.5).is_err());
    assert!(G2Process::new(0.1, 0.01, 0.2, -0.02, 0.5).is_err());
    for rho in [-1.01, 1.01] {
        assert!(G2Process::new(0.1, 0.01, 0.2, 0.02, rho).is_err());
    }
}

#[test]
fn fallible_dynamics_reject_invalid_dimensions_times_and_nonfinite_values() {
    let p = G2Process::new(0.1, 0.01, 0.2, 0.02, 0.5).unwrap();
    for x in [
        Array::new(),
        Array::from([0.0]),
        Array::from([0.0, 0.0, 0.0]),
        Array::from([f64::NAN, 0.0]),
        Array::from([0.0, f64::INFINITY]),
    ] {
        assert!(p.drift(0.0, &x).is_err());
        assert!(p.diffusion(0.0, &x).is_err());
        assert!(p.expectation(0.0, &x, 1.0).is_err());
        assert!(p.std_deviation(0.0, &x, 1.0).is_err());
        assert!(p.covariance(0.0, &x, 1.0).is_err());
        assert!(p.evolve(0.0, &zero(), 1.0, &x).is_err());
    }
    for t in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(p.drift(t, &zero()).is_err());
        assert!(p.diffusion(t, &zero()).is_err());
        assert!(p.expectation(t, &zero(), 1.0).is_err());
        assert!(p.covariance(0.0, &zero(), t).is_err());
    }
    assert!(p.expectation(f64::MAX, &zero(), f64::MAX).is_err());
}

#[test]
fn unrepresentable_results_return_errors() {
    let p = G2Process::new(-f64::MAX, 0.01, 0.2, 0.02, 0.5).unwrap();
    assert!(p.drift(0.0, &Array::from([2.0, 0.0])).is_err());
    assert!(p.expectation(0.0, &Array::from([1.0, 0.0]), 1.0).is_err());
    assert!(p.std_deviation(0.0, &zero(), 1.0).is_err());
    let p = G2Process::new(0.0, f64::MAX, 0.0, 0.02, 0.5).unwrap();
    assert!(p.covariance(0.0, &zero(), 1.0).is_err());
    assert!(
        p.evolve(0.0, &zero(), 1.0, &Array::from([f64::MAX, 0.0]))
            .is_err()
    );
}

#[test]
fn regular_exact_steps_match_native_quantlib_1_43_fixtures() {
    for line in include_str!("data/g2_native.csv").lines().skip(1) {
        let row: Vec<f64> = line.split(',').map(|v| v.parse().unwrap()).collect();
        let p = G2Process::new(row[0], row[1], row[2], row[3], row[4]).unwrap();
        let x = Array::from([row[7], row[8]]);
        let e = p.expectation(row[5], &x, row[6]).unwrap();
        close(e[0], row[9]);
        close(e[1], row[10]);
        let c = p.covariance(row[5], &x, row[6]).unwrap();
        let s = p.std_deviation(row[5], &x, row[6]).unwrap();
        for i in 0..2 {
            for j in 0..2 {
                close(c[(i, j)], row[11 + 2 * i + j]);
                close(s[(i, j)], row[15 + 2 * i + j]);
            }
        }
    }
}

#[test]
fn singular_tiny_steps_retain_the_second_independent_shock() {
    let p = G2Process::new(0.1, 0.03, 0.2, 0.007, 1.0).unwrap();
    let dt = 1e-8;
    let s = p.std_deviation(0.0, &zero(), dt).unwrap();
    let leading = 0.007 * dt.sqrt() * (0.1 * dt) / 12.0_f64.sqrt();
    assert!((s[(1, 1)] / leading - 1.0).abs() < 3e-9);
    let shock = p
        .evolve(0.0, &zero(), dt, &Array::from([0.0, 1.0]))
        .unwrap();
    assert!(shock[1] > 0.0);
    assert_eq!(shock[1], s[(1, 1)]);
}

#[test]
fn singular_close_rates_retain_independent_shocks_at_ordinary_times() {
    for rate in [0.1, 1.0, 10.0, 100.0] {
        let difference = rate * 1e-7;
        let p = G2Process::new(rate, 0.03, rate + difference, 0.007, 1.0).unwrap();
        let s = p.std_deviation(0.0, &zero(), 1.0).unwrap();
        let csch = if rate < 100.0 { 1.0 / rate.sinh() } else { 0.0 };
        let leading = 0.007
            * ((-(-2.0 * rate).exp_m1()) / (2.0 * rate)).sqrt()
            * difference
            * 0.5
            * (1.0 / (rate * rate) - csch * csch).sqrt();
        assert!(s[(1, 1)] > 0.0);
        assert!((s[(1, 1)] / leading - 1.0).abs() < 2e-7);
    }
}

#[test]
fn near_singular_roots_match_90_digit_decimal_regressions() {
    for (a, b, rho, dt, expected) in [
        (1e-18, 2e-18, 0.999999999999, 1.0, 1.1313583358943374e-8),
        (0.1, 0.2, 1.0, 1e-8, 2.3094010744491024e-16),
        (0.1, 0.1000001, -1.0, 1.0, 2.196404730112083e-10),
        (0.0, 0.02, 1.0, 1.0, 4.57290527571399e-5),
        (0.012, -0.006, -1.0, 1.0, 4.1693864126915885e-5),
    ] {
        let p = G2Process::new(a, 0.01, b, 0.008, rho).unwrap();
        let actual = p.std_deviation(0.0, &zero(), dt).unwrap()[(1, 1)];
        assert!(
            (actual / expected - 1.0).abs() < 2e-12,
            "{actual:e} != {expected:e}"
        );
    }
}

#[test]
fn finite_cross_moments_survive_correlation_and_loading_underflow() {
    for (eta, dt, cross, loading) in [
        (
            1e154,
            800.0,
            4.0000000000000003e-44,
            2.0748631928110647e-191,
        ),
        (1e154, 740.0, 3.7e-44, 2.1917959166182907e-165),
        (1e-3, 800.0, 4e-201, 0.0),
    ] {
        for sign in [-1.0, 1.0] {
            let p = G2Process::new(-1.0, 1e-200, 1.0, eta, sign * 0.5).unwrap();
            let c = p.covariance(0.0, &zero(), dt).unwrap();
            let s = p.std_deviation(0.0, &zero(), dt).unwrap();
            assert!((c[(0, 1)] / (sign * cross) - 1.0).abs() < 2e-12);
            assert_eq!(c[(0, 1)], c[(1, 0)]);
            if loading == 0.0 {
                assert_eq!(s[(1, 0)], 0.0);
            } else {
                assert!((s[(1, 0)] / (sign * loading) - 1.0).abs() < 2e-12);
            }
        }
    }
}

#[test]
fn ordinary_singular_covariance_preserves_exact_gram_symmetry_and_psd() {
    for rho in [-1.0, 1.0] {
        let p = G2Process::new(0.0, 1.0, 0.0, 1.0, rho).unwrap();
        let s = p.std_deviation(0.0, &zero(), 0.3).unwrap();
        let c = p.covariance(0.0, &zero(), 0.3).unwrap();
        assert_eq!(c, &s * &s.transpose());
        assert_eq!(c[(0, 0)], c[(0, 1)].abs());
        assert_eq!(c[(1, 1)], c[(0, 1)].abs());
        assert_eq!(c[(0, 0)] * c[(1, 1)] - c[(0, 1)] * c[(1, 0)], 0.0);
    }
}
