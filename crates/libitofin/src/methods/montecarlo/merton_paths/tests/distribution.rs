use super::*;

#[test]
fn jump_counts_match_poisson_mean_variance_and_multiple_event_mass() {
    let mean: Real = 2.0;
    let mut draws = Draws::new(42, mean).unwrap();
    let samples = 80_000;
    let mut sum = 0.0;
    let mut sum2 = 0.0;
    let mut multiple = 0;
    for _ in 0..samples {
        let (_, count, _) = draws.next().unwrap();
        sum += count;
        sum2 += count * count;
        if count >= 2.0 {
            multiple += 1;
        }
    }
    let empirical_mean = sum / samples as Real;
    let variance = sum2 / samples as Real - empirical_mean * empirical_mean;
    assert_close(empirical_mean, mean, 0.025);
    assert_close(variance, mean, 0.05);
    assert_close(
        multiple as Real / samples as Real,
        1.0 - (-mean).exp() * (1.0 + mean),
        0.007,
    );
}

#[test]
fn terminal_log_moments_and_arithmetic_growth_match_compensated_model() {
    let r = MertonRequest {
        paths: 100_000,
        terminal_only: true,
        steps: 3,
        ..request()
    };
    let output = merton_paths(&r).unwrap();
    let log_values: Vec<_> = output.iter().map(|s| (s / r.spot).ln()).collect();
    let empirical_mean = log_values.iter().sum::<Real>() / r.paths as Real;
    let empirical_variance = log_values
        .iter()
        .map(|x| (x - empirical_mean).powi(2))
        .sum::<Real>()
        / r.paths as Real;
    let kappa = (r.log_mean_jump + 0.5 * r.log_jump_volatility.powi(2)).exp_m1();
    let mean = r.horizon
        * (r.drift - r.jump_intensity * kappa - 0.5 * r.volatility.powi(2)
            + r.jump_intensity * r.log_mean_jump);
    let variance = r.horizon
        * (r.volatility.powi(2)
            + r.jump_intensity * (r.log_mean_jump.powi(2) + r.log_jump_volatility.powi(2)));
    assert_close(empirical_mean, mean, 0.008);
    assert_close(empirical_variance, variance, 0.008);
    assert_close(
        output.iter().sum::<Real>() / r.paths as Real,
        r.spot * (r.drift * r.horizon).exp(),
        0.8,
    );
}
