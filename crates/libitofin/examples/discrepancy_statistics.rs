use libitofin::math::statistics::DiscrepancyStatistics;

fn main() -> libitofin::errors::QlResult<()> {
    let mut statistics = DiscrepancyStatistics::new(2)?;
    for point in [[0.25, 0.25], [0.75, 0.75], [0.25, 0.75], [0.75, 0.25]] {
        statistics.add(&point)?;
    }
    let discrepancy = statistics.discrepancy()?;
    assert!((discrepancy - (71.0_f64 / 4608.0).sqrt()).abs() < 1e-12);
    println!("uncentered L2 discrepancy: {discrepancy}");
    Ok(())
}
