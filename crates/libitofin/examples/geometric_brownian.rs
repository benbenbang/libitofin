use libitofin::processes::GeometricBrownianMotionProcess;
use libitofin::stochasticprocess::StochasticProcess1D;

fn main() -> libitofin::errors::QlResult<()> {
    let process = GeometricBrownianMotionProcess::new(100.0, 0.05, 0.20)?;
    println!("initial state: {:.10}", process.x0()?);
    println!("mu: {:.10}", process.mu());
    println!("volatility: {:.10}", process.volatility());
    println!("arithmetic drift: {:.10}", process.drift(0.0, 100.0)?);
    println!(
        "arithmetic diffusion: {:.10}",
        process.diffusion(0.0, 100.0)?
    );
    println!(
        "Euler expectation: {:.10}",
        process.expectation(0.0, 100.0, 0.25)?
    );
    println!(
        "Euler variance: {:.10}",
        process.variance(0.0, 100.0, 0.25)?
    );
    println!(
        "signed Euler deviation: {:.10}",
        process.std_deviation(0.0, -100.0, 0.25)?
    );
    println!(
        "Euler transition: {:.10}",
        process.evolve(0.0, 100.0, 0.25, -0.25)?
    );
    println!(
        "Euler crossing zero: {:.10}",
        process.evolve(0.0, 100.0, 1.0, -6.0)?
    );
    Ok(())
}
