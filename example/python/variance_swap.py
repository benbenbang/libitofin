"""Price a spot-start annualized variance swap with a finite option strip."""

# itofin library
from itofin import Settings
from itofin.instruments import Position, VarianceSwap
from itofin.pricingengines import ReplicatingVarianceSwapEngine
from itofin.processes import BlackScholesProcess
from itofin.time import Date, DayCounter


def main() -> None:
    """Print annualized variance, cash NPV and purchased option count."""
    today = Date(5, 10, 2026)
    settings = Settings()
    settings.set_evaluation_date(today)
    process = BlackScholesProcess(100.0, 0.03, 0.0, 0.20, today, DayCounter.actual365_fixed())
    engine = ReplicatingVarianceSwapEngine(process, list(range(100, 151, 5)), list(range(50, 101, 5)), dk=5.0)
    swap = VarianceSwap(Position.Long, 0.04, 50_000.0, today, today + 365, settings)
    swap.set_engine(engine)
    print(f"annualized variance: {swap.variance():.10f}")
    print(f"NPV: {swap.npv():.10f}")
    print(f"purchased option weights: {len(swap.option_weights())}")


if __name__ == "__main__":
    main()
