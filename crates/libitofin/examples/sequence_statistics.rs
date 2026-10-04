use libitofin::math::statistics::SequenceStatistics;

fn main() -> libitofin::errors::QlResult<()> {
    let mut statistics = SequenceStatistics::new(2)?;
    statistics.add_weighted(&[1.0, 4.0], 1.0)?;
    statistics.add_weighted(&[3.0, 2.0], 3.0)?;
    statistics.add_weighted(&[100.0, -20.0], 0.0)?;
    let mean = statistics.mean()?;
    let covariance = statistics.covariance()?;
    let correlation = statistics.correlation()?;
    let maximum = statistics.max()?;
    assert_eq!(mean, [2.5, 2.5]);
    for (actual, expected) in covariance.iter().zip([1.125, -1.125, -1.125, 1.125]) {
        assert!((actual - expected).abs() < 1e-12);
    }
    for (actual, expected) in correlation.iter().zip([1.0, -1.0, -1.0, 1.0]) {
        assert!((actual - expected).abs() < 1e-12);
    }
    assert_eq!(maximum, [100.0, 4.0]);
    println!("mean: {mean:?}");
    println!("covariance: {covariance:?}");
    println!("correlation: {correlation:?}");
    println!("maximum: {maximum:?}");
    Ok(())
}
