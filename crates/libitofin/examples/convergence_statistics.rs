use libitofin::math::statistics::ConvergenceStatistics;

fn main() -> libitofin::errors::QlResult<()> {
    let mut statistics = ConvergenceStatistics::new();
    statistics.add_batch(&[2.0, 100.0, 8.0, 12.0], Some(&[1.0, 0.0, 3.0, 2.0]))?;
    let table = statistics.convergence_table();
    assert_eq!(table.len(), 2);
    assert_eq!((table[0].samples, table[0].mean), (1, 2.0));
    assert_eq!((table[1].samples, table[1].mean), (3, 6.5));
    assert_eq!(statistics.samples(), 4);
    assert!((statistics.mean()? - 25.0 / 3.0).abs() < 1e-12);
    println!("checkpoint table: {table:?}");
    println!("current mean: {}", statistics.mean()?);
    statistics.reset();
    assert!(statistics.convergence_table().is_empty());
    Ok(())
}
