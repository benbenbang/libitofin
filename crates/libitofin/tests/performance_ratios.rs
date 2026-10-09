use libitofin::math::statistics::{sharpe_ratio, sortino_ratio, target_downside_deviation};

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-13 * expected.abs().max(1.0),
        "{actual} != {expected}"
    );
}

#[test]
fn performance_ratios_hand_denominators_and_targets() {
    let returns = [0.02, -0.01, 0.03, -0.02];
    close(
        target_downside_deviation(&returns, 0.0).unwrap(),
        (0.0005_f64 / 4.0).sqrt(),
    );
    close(
        sharpe_ratio(&returns, 0.0, 1.0).unwrap(),
        0.005 / (0.0017_f64 / 3.0).sqrt(),
    );
    close(
        sortino_ratio(&returns, 0.0, 1.0).unwrap(),
        1.0 / 5.0_f64.sqrt(),
    );
    close(
        sortino_ratio(&returns, 0.0, 12.0).unwrap(),
        (12.0_f64 / 5.0).sqrt(),
    );
    close(
        sharpe_ratio(&returns, 0.01, 1.0).unwrap(),
        -0.005 / (0.0017_f64 / 3.0).sqrt(),
    );
    close(
        target_downside_deviation(&returns, 0.01).unwrap(),
        (0.0013_f64 / 4.0).sqrt(),
    );
    close(
        sortino_ratio(&returns, 0.01, 1.0).unwrap(),
        -0.005 / (0.0013_f64 / 4.0).sqrt(),
    );
    close(
        sortino_ratio(&[-0.01, 0.03], 0.0, 1.0).unwrap(),
        2.0_f64.sqrt(),
    );
    close(sortino_ratio(&[-0.01, -0.01], 0.0, 1.0).unwrap(), -1.0);
    assert_eq!(target_downside_deviation(&[0.01, 0.02], 0.0).unwrap(), 0.0);
    assert_eq!(target_downside_deviation(&[-0.02], 0.0).unwrap(), 0.02);
    close(sharpe_ratio(&[1.0, 2.0, 3.0], 0.0, 4.0).unwrap(), 4.0);
    assert_eq!(sortino_ratio(&[-1.0, 1.0], 0.0, 1.0).unwrap(), 0.0);
}

#[test]
fn performance_ratios_scale_extremes_and_tail() {
    for scale in [1e-300, 1.0, 1e300] {
        let returns = [-scale, scale, 2.0 * scale];
        close(
            sharpe_ratio(&returns, 0.0, 1.0).unwrap(),
            2.0 / 21.0_f64.sqrt(),
        );
        close(
            sortino_ratio(&returns, 0.0, 1.0).unwrap(),
            2.0 / 3.0_f64.sqrt(),
        );
    }
    close(
        target_downside_deviation(&[1e200, -1.0], 0.0).unwrap(),
        0.5_f64.sqrt(),
    );
    close(
        sortino_ratio(&[1e200, -1.0], 0.0, 1.0).unwrap(),
        1e200 / 2.0_f64.sqrt(),
    );
    close(
        sharpe_ratio(&[1.0, 2.0, 3.0], 1e16, 1.0).unwrap(),
        2.0 - 1e16,
    );
    close(
        sharpe_ratio(&[1e16, 1e16 + 2.0, 1e16 + 4.0], 0.0, 1.0).unwrap(),
        5e15 + 1.0,
    );
    close(sharpe_ratio(&[-f64::MAX, f64::MAX], 0.0, 1.0).unwrap(), 0.0);
    assert!(sharpe_ratio(&[1.0; 7], 0.0, 1.0).is_err());
    assert!(sharpe_ratio(&[1.0; 10], 0.0, 1.0).is_err());
    close(
        sharpe_ratio(&[1e16, 1e16 + 2.0], 1e16, 1.0).unwrap(),
        1.0 / 2.0_f64.sqrt(),
    );
    close(
        sortino_ratio(&[1e16, 1e16 + 2.0], 1e16 + 2.0, 1.0).unwrap(),
        -1.0 / 2.0_f64.sqrt(),
    );
    let returns = [-0.4, 0.01, 0.01, 0.01];
    close(target_downside_deviation(&returns, 0.0).unwrap(), 0.2);
    close(
        sortino_ratio(&returns, 0.0, 252.0).unwrap(),
        -0.4625 * 252.0_f64.sqrt(),
    );
}

#[test]
fn performance_ratios_invalid_inputs() {
    for returns in [
        vec![],
        vec![1.0],
        vec![1.0, f64::NAN],
        vec![1.0, f64::INFINITY],
    ] {
        assert!(sharpe_ratio(&returns, 0.0, 12.0).is_err());
        assert!(sortino_ratio(&returns, 0.0, 12.0).is_err());
    }
    for frequency in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(sharpe_ratio(&[-1.0, 2.0], 0.0, frequency).is_err());
        assert!(sortino_ratio(&[-1.0, 2.0], 0.0, frequency).is_err());
    }
    for target in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(target_downside_deviation(&[-1.0, 2.0], target).is_err());
        assert!(sharpe_ratio(&[-1.0, 2.0], target, 12.0).is_err());
        assert!(sortino_ratio(&[-1.0, 2.0], target, 12.0).is_err());
    }
    for flat in [[0.0, 0.0], [0.01, 0.01], [-0.01, -0.01]] {
        assert!(sharpe_ratio(&flat, 0.0, 12.0).is_err());
    }
    assert!(sortino_ratio(&[0.01, 0.02], 0.0, 12.0).is_err());
    assert!(sortino_ratio(&[0.0, 0.0], 0.0, 12.0).is_err());
    assert!(target_downside_deviation(&[], 0.0).is_err());
    assert!(target_downside_deviation(&[f64::MAX], -f64::MAX).is_err());
    assert!(sharpe_ratio(&[f64::MAX, 0.0], -f64::MAX, 1.0).is_err());
    assert!(sortino_ratio(&[f64::MAX, 0.0], -f64::MAX, 1.0).is_err());
}
