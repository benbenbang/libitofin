"""Estimate spot-start annualized variance from integrated log-price diffusion."""

# itofin library
from itofin import Settings
from itofin.instruments import Position, VarianceSwap
from itofin.pricingengines import MCVarianceSwapEngine
from itofin.processes import BlackScholesProcess
from itofin.time import Date, DayCounter


def main() -> None:
    """Print variance and distinct sampling errors for a constant-volatility market."""
    today = Date(6, 10, 2026)
    settings = Settings()
    settings.set_evaluation_date(today)
    process = BlackScholesProcess(100.0, 0.03, 0.0, 0.20, today, DayCounter.actual365_fixed())
    engine = MCVarianceSwapEngine(process, steps=252, samples=1023, seed=42)
    swap = VarianceSwap(Position.Long, 0.04, 50_000.0, today, today + 365, settings)
    swap.set_engine(engine)
    print(f"annualized variance: {swap.variance():.10f}")
    print(f"NPV: {swap.npv():.10f}")
    print(f"variance standard error: {swap.variance_error():.10f}")
    print(f"NPV error estimate: {swap.error_estimate():.10f}")
    print(f"samples: {swap.samples()}")


if __name__ == "__main__":
    main()
