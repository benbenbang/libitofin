use super::*;
use crate::methods::montecarlo::simulation_kernel::gaussian_draws;

pub(super) fn request() -> GjrRequest {
    GjrRequest {
        spot: 100.0,
        daily_variance: 0.04 / 252.0,
        risk_free_rate: 0.03,
        dividend_yield: 0.01,
        omega: 2e-6,
        alpha: 0.04,
        beta: 0.90,
        gamma: 0.07,
        lambda: 0.0,
        days_per_year: 252.0,
        horizon: 4.0 / 252.0,
        steps: 4,
        paths: 3,
        seed: 42,
        discretization: GjrGarchDiscretization::PartialTruncation,
        terminal_only: false,
    }
}

fn parameters(r: &GjrRequest) -> GjrGarchParameters {
    GjrGarchParameters {
        v0: r.daily_variance,
        omega: r.omega,
        alpha: r.alpha,
        beta: r.beta,
        gamma: r.gamma,
        lambda: r.lambda,
        days_per_year: r.days_per_year,
    }
}

fn schemes() -> [GjrGarchDiscretization; 3] {
    [
        GjrGarchDiscretization::PartialTruncation,
        GjrGarchDiscretization::FullTruncation,
        GjrGarchDiscretization::Reflection,
    ]
}

#[test]
fn replay_terminal_and_path_prefix_have_identical_bits() {
    for discretization in schemes() {
        let r = GjrRequest {
            discretization,
            ..request()
        };
        let full = gjr_paths(&r).unwrap();
        assert_eq!(full.len(), output_len(&r).unwrap());
        assert_eq!(full, gjr_paths(&r).unwrap());
        let stride = (r.steps + 1) * 2;
        for path in full.chunks_exact(stride) {
            assert_eq!(&path[..2], &[r.spot, r.daily_variance * r.days_per_year]);
        }
        let terminal = gjr_paths(&GjrRequest {
            terminal_only: true,
            ..r
        })
        .unwrap();
        for (path, pair) in terminal.chunks_exact(2).enumerate() {
            assert_eq!(pair, &full[path * stride + stride - 2..(path + 1) * stride]);
        }
        let prefix = gjr_paths(&GjrRequest { paths: 1, ..r }).unwrap();
        assert_eq!(prefix, full[..stride]);
        let other = gjr_paths(&GjrRequest { seed: 91, ..r }).unwrap();
        assert_ne!(full, other);
    }
}

#[test]
fn seeded_draw_order_matches_scalar_native_gaussian_stream() {
    let r = request();
    let full = gjr_paths(&r).unwrap();
    let coefficients = parameters(&r).coefficients().unwrap();
    let draws = gaussian_draws(r.paths * r.steps * 2, r.seed).unwrap();
    let dt = r.horizon / r.steps as Real;
    let stride = (r.steps + 1) * 2;
    for path in 0..r.paths {
        let mut state = [r.spot, coefficients.initial_variance()];
        for step in 0..r.steps {
            let index = (path * r.steps + step) * 2;
            state = coefficients
                .evolve(
                    state,
                    dt,
                    r.risk_free_rate - r.dividend_yield,
                    [draws[index], draws[index + 1]],
                    r.discretization,
                )
                .unwrap();
            let output = path * stride + (step + 1) * 2;
            assert_eq!(state, full[output..output + 2]);
        }
    }
}

#[test]
fn zero_horizon_preserves_initial_states_in_all_modes_and_schemes() {
    for discretization in schemes() {
        for terminal_only in [false, true] {
            let r = GjrRequest {
                horizon: 0.0,
                discretization,
                terminal_only,
                ..request()
            };
            let full = gjr_paths(&r).unwrap();
            assert_eq!(full.len(), output_len(&r).unwrap());
            for pair in full.chunks_exact(2) {
                assert_eq!(pair, &[r.spot, r.daily_variance * r.days_per_year]);
            }
        }
    }
}

#[test]
fn raw_negative_variance_is_retained_and_schemes_diverge() {
    let r = GjrRequest {
        daily_variance: 0.0625 / 252.0,
        beta: 0.0,
        alpha: 0.0,
        gamma: 0.0,
        omega: 0.0,
        horizon: 4.0 / 252.0,
        steps: 2,
        paths: 1,
        ..request()
    };
    let partial = gjr_paths(&r).unwrap();
    let full = gjr_paths(&GjrRequest {
        discretization: GjrGarchDiscretization::FullTruncation,
        ..r
    })
    .unwrap();
    let reflection = gjr_paths(&GjrRequest {
        discretization: GjrGarchDiscretization::Reflection,
        ..r
    })
    .unwrap();
    assert!(partial[3] < 0.0);
    assert_eq!(partial[3], full[3]);
    assert_eq!(partial[3], reflection[3]);
    assert!(partial[5] > 0.0);
    assert_eq!(full[5], full[3]);
    assert!(reflection[5] < 0.0);
    assert_ne!(partial[4], reflection[4]);
    assert_eq!(partial[4], full[4]);
}

#[test]
fn one_step_joint_moments_match_independent_analytic_values() {
    let r = GjrRequest {
        steps: 1,
        paths: 100_000,
        horizon: 1.0 / 252.0,
        terminal_only: true,
        ..request()
    };
    let values = gjr_paths(&r).unwrap();
    let n = r.paths as Real;
    let variance = r.daily_variance * r.days_per_year;
    let dt = r.horizon;
    let expected_log_mean =
        r.spot.ln() + (r.risk_free_rate - r.dividend_yield - 0.5 * variance) * dt;
    let expected_log_variance = variance * dt;
    let variance_drift = r.days_per_year.powi(2) * r.omega
        + r.days_per_year * (r.beta + r.alpha + 0.5 * r.gamma - 1.0) * variance;
    let expected_variance_mean = variance + variance_drift * dt;
    let expected_variance_variance = dt
        * variance.powi(2)
        * r.days_per_year
        * (2.0 * r.alpha.powi(2) + 1.25 * r.gamma.powi(2) + 2.0 * r.alpha * r.gamma);
    let expected_covariance = -2.0
        * r.gamma
        * (r.days_per_year / std::f64::consts::TAU).sqrt()
        * dt
        * variance
        * variance.sqrt();
    let log_mean = values.chunks_exact(2).map(|v| v[0].ln()).sum::<Real>() / n;
    let variance_mean = values.chunks_exact(2).map(|v| v[1]).sum::<Real>() / n;
    let log_variance = values
        .chunks_exact(2)
        .map(|v| (v[0].ln() - log_mean).powi(2))
        .sum::<Real>()
        / n;
    let variance_variance = values
        .chunks_exact(2)
        .map(|v| (v[1] - variance_mean).powi(2))
        .sum::<Real>()
        / n;
    let covariance = values
        .chunks_exact(2)
        .map(|v| (v[0].ln() - log_mean) * (v[1] - variance_mean))
        .sum::<Real>()
        / n;
    assert!((log_mean - expected_log_mean).abs() < 6.0 * (expected_log_variance / n).sqrt());
    assert!(
        (variance_mean - expected_variance_mean).abs()
            < 6.0 * (expected_variance_variance / n).sqrt()
    );
    assert!(
        (log_variance - expected_log_variance).abs()
            < 6.0 * expected_log_variance * (2.0 / n).sqrt()
    );
    assert!(
        (variance_variance - expected_variance_variance).abs()
            < 6.0 * expected_variance_variance * (2.0 / n).sqrt()
    );
    assert!(
        (covariance - expected_covariance).abs()
            < 6.0
                * ((expected_log_variance * expected_variance_variance
                    + expected_covariance.powi(2))
                    / n)
                    .sqrt()
    );
}

#[test]
fn finite_horizon_nonstationary_and_negative_gamma_parameters_are_valid() {
    for r in [
        GjrRequest {
            beta: 1.2,
            ..request()
        },
        GjrRequest {
            gamma: -0.02,
            lambda: 0.3,
            ..request()
        },
    ] {
        assert!(gjr_paths(&r).unwrap().iter().all(|value| value.is_finite()));
    }
}

#[test]
fn zero_initial_variance_and_zero_intercept_leave_only_carry() {
    for discretization in schemes() {
        let r = GjrRequest {
            daily_variance: 0.0,
            omega: 0.0,
            discretization,
            paths: 1,
            seed: Natural::MAX,
            ..request()
        };
        let values = gjr_paths(&r).unwrap();
        for (step, state) in values.chunks_exact(2).enumerate() {
            let expected = r.spot
                * ((r.risk_free_rate - r.dividend_yield) * step as Real * r.horizon
                    / r.steps as Real)
                    .exp();
            assert!((state[0] - expected).abs() < 1e-12);
            assert_eq!(state[1], 0.0);
        }
        assert_eq!(values, gjr_paths(&GjrRequest { seed: 91, ..r }).unwrap());
    }
}
