use super::*;
use crate::methods::montecarlo::simulation_kernel::{GbmRequest, gbm_paths};

fn request() -> MertonRequest {
    MertonRequest {
        spot: 100.0,
        drift: 0.05,
        volatility: 0.2,
        jump_intensity: 1.0,
        log_mean_jump: -0.1,
        log_jump_volatility: 0.3,
        horizon: 1.0,
        steps: 4,
        paths: 3,
        seed: 42,
        terminal_only: false,
    }
}

#[test]
fn zero_jump_matches_scalar_gbm_bits_in_both_volatility_branches() {
    for volatility in [0.0, 0.2, 0.731] {
        for seed in [1, 42, Natural::MAX] {
            for terminal_only in [false, true] {
                let r = MertonRequest {
                    jump_intensity: 0.0,
                    volatility,
                    seed,
                    terminal_only,
                    steps: 7,
                    horizon: 1.17,
                    ..request()
                };
                let initial = [r.spot];
                let drift = [r.drift];
                let volatility = [r.volatility];
                let gbm = GbmRequest {
                    initial: &initial,
                    drift: &drift,
                    volatility: &volatility,
                    correlation: &[1.0],
                    horizon: r.horizon,
                    steps: r.steps,
                    paths: r.paths,
                    seed: r.seed,
                    terminal_only,
                };
                let expected = gbm_paths(&gbm).unwrap();
                let actual = merton_paths(&r).unwrap();
                assert_eq!(
                    actual.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                    expected.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
                );
            }
        }
    }
}

#[test]
fn terminal_layout_and_reproducibility_share_full_path_bits() {
    let r = request();
    let full = merton_paths(&r).unwrap();
    assert_eq!(full.len(), r.paths * (r.steps + 1));
    assert_eq!(full, merton_paths(&r).unwrap());
    assert_ne!(
        full,
        merton_paths(&MertonRequest { seed: 91, ..r }).unwrap()
    );
    let terminal = merton_paths(&MertonRequest {
        terminal_only: true,
        ..r
    })
    .unwrap();
    for (path, &value) in terminal.iter().enumerate() {
        assert_eq!(full[path * (r.steps + 1)], r.spot);
        assert_eq!(
            value.to_bits(),
            full[path * (r.steps + 1) + r.steps].to_bits()
        );
    }
}

#[test]
fn zero_horizon_returns_initial_spot_without_transition_arithmetic() {
    for terminal_only in [false, true] {
        let r = MertonRequest {
            horizon: 0.0,
            terminal_only,
            drift: Real::MAX,
            volatility: 1e150,
            ..request()
        };
        assert!(
            merton_paths(&r)
                .unwrap()
                .iter()
                .all(|x| x.to_bits() == r.spot.to_bits())
        );
    }
}

#[test]
fn all_zero_draw_coefficients_still_advance_three_streams() {
    let mut zero = Draws::new(42, 0.0).unwrap();
    let mut active = Draws::new(42, 2.0).unwrap();
    for _ in 0..20 {
        let (zd0, n0, zj0) = zero.next().unwrap();
        let (zd, _, zj) = active.next().unwrap();
        assert_eq!(n0, 0.0);
        assert_eq!(zd0, zd);
        assert_eq!(zj0, zj);
    }
    assert_eq!(zero.counts.next_u32(), active.counts.next_u32());
}

#[test]
fn derived_seed_wraps_are_nonzero_and_pin_stream_roles() {
    assert_eq!(shifted_seed(42, 0x9e37_79b9), 2654435811);
    assert_eq!(shifted_seed(42, 0xbb67_ae85), 3144134319);
    for seed in [1, 42, Natural::MAX] {
        assert!(shifted_seed(seed, 0x9e37_79b9) > 0);
        assert!(shifted_seed(seed, 0xbb67_ae85) > 0);
        assert_ne!(
            shifted_seed(seed, 0x9e37_79b9),
            shifted_seed(seed, 0xbb67_ae85)
        );
    }
    assert_eq!(shifted_seed(Natural::MAX, 0x9e37_79b9), 0x9e37_79b9);
    assert_eq!(shifted_seed(Natural::MAX, 0xbb67_ae85), 0xbb67_ae85);
}

#[test]
fn invalid_domains_and_dimension_overflows_are_errors() {
    for r in [
        MertonRequest {
            spot: 0.0,
            ..request()
        },
        MertonRequest {
            spot: Real::INFINITY,
            ..request()
        },
        MertonRequest {
            drift: Real::NAN,
            ..request()
        },
        MertonRequest {
            volatility: -0.1,
            ..request()
        },
        MertonRequest {
            jump_intensity: -1.0,
            ..request()
        },
        MertonRequest {
            log_mean_jump: Real::NAN,
            ..request()
        },
        MertonRequest {
            log_jump_volatility: -1.0,
            ..request()
        },
        MertonRequest {
            horizon: -1.0,
            ..request()
        },
        MertonRequest {
            steps: 0,
            ..request()
        },
        MertonRequest {
            paths: 0,
            ..request()
        },
        MertonRequest {
            seed: 0,
            ..request()
        },
        MertonRequest {
            steps: Size::MAX,
            ..request()
        },
        MertonRequest {
            paths: Size::MAX,
            ..request()
        },
        MertonRequest {
            steps: Size::MAX / 2,
            paths: 1,
            terminal_only: true,
            ..request()
        },
    ] {
        assert!(merton_paths(&r).is_err());
    }
    assert!(
        output_len(&MertonRequest {
            steps: Size::MAX,
            ..request()
        })
        .is_err()
    );
    assert!(
        output_len(&MertonRequest {
            paths: Size::MAX,
            ..request()
        })
        .is_err()
    );
    assert_eq!(
        output_len(&MertonRequest {
            terminal_only: true,
            steps: Size::MAX,
            ..request()
        })
        .unwrap(),
        request().paths
    );
}

#[test]
fn unrepresentable_parameters_and_inverse_count_domain_are_errors() {
    for r in [
        MertonRequest {
            volatility: Real::MAX,
            ..request()
        },
        MertonRequest {
            log_jump_volatility: Real::MAX,
            ..request()
        },
        MertonRequest {
            log_mean_jump: 1000.0,
            ..request()
        },
        MertonRequest {
            log_mean_jump: -1000.0,
            ..request()
        },
        MertonRequest {
            log_mean_jump: 700.0,
            jump_intensity: Real::MAX,
            ..request()
        },
        MertonRequest {
            jump_intensity: Real::MAX,
            horizon: Real::MAX,
            ..request()
        },
        MertonRequest {
            jump_intensity: 800.0,
            steps: 1,
            ..request()
        },
        MertonRequest {
            horizon: Real::from_bits(1),
            ..request()
        },
        MertonRequest {
            jump_intensity: Real::from_bits(1),
            horizon: 0.1,
            steps: 1,
            ..request()
        },
    ] {
        assert!(merton_paths(&r).is_err());
    }
}

#[test]
fn simulated_spot_overflow_underflow_and_mid_path_failure_return_errors() {
    let jumps = MertonRequest {
        volatility: 0.0,
        log_mean_jump: -700.0,
        log_jump_volatility: 0.0,
        paths: 1,
        ..request()
    };
    assert!(
        merton_paths(&MertonRequest {
            steps: 2,
            horizon: 0.5,
            ..jumps
        })
        .is_ok()
    );
    assert!(merton_paths(&jumps).is_err());
    assert!(
        merton_paths(&MertonRequest {
            terminal_only: true,
            ..jumps
        })
        .is_err()
    );
    for (drift, steps) in [(2000.0, 1), (-2000.0, 1), (1000.0, 4), (-1000.0, 4)] {
        let r = MertonRequest {
            drift,
            steps,
            jump_intensity: 0.0,
            volatility: 0.0,
            ..request()
        };
        assert!(merton_paths(&r).is_err());
        assert!(
            merton_paths(&MertonRequest {
                terminal_only: true,
                ..r
            })
            .is_err()
        );
    }
}
