use libitofin::errors::QlResult;
use libitofin::methods::montecarlo::gjr_paths::{GjrRequest, gjr_paths};
use libitofin::processes::GjrGarchDiscretization;

fn main() -> QlResult<()> {
    let request = GjrRequest {
        spot: 100.0,
        daily_variance: 0.04 / 252.0,
        risk_free_rate: 0.05,
        dividend_yield: 0.02,
        omega: 2e-6,
        alpha: 0.03,
        beta: 0.90,
        gamma: 0.04,
        lambda: 0.1,
        days_per_year: 252.0,
        horizon: 1.0,
        steps: 252,
        paths: 8,
        seed: 42,
        discretization: GjrGarchDiscretization::FullTruncation,
        terminal_only: false,
    };
    let full = gjr_paths(&request)?;
    let terminal = gjr_paths(&GjrRequest {
        terminal_only: true,
        ..request
    })?;
    for path in 0..request.paths {
        let offset = (path * (request.steps + 1) + request.steps) * 2;
        assert_eq!(&full[offset..offset + 2], &terminal[path * 2..path * 2 + 2]);
    }
    println!(
        "{} paths, {} times, 2 components",
        request.paths,
        request.steps + 1
    );
    Ok(())
}
