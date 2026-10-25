"""European percentage-strike option using the existing finite-difference engine."""

# itofin library
from itofin import Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.pricingengines import FdBlackScholesVanillaEngine
from itofin.processes import BlackScholesProcess
from itofin.time import Date, DayCounter


def main():
    reference = Date(15, 6, 2026)
    settings = Settings()
    settings.set_evaluation_date(reference)
    process = BlackScholesProcess(100.0, 0.05, 0.02, 0.2, reference, DayCounter.actual360())
    option = VanillaOption.percentage_strike(OptionType.Call, 0.8, reference + 360, settings)
    engine = FdBlackScholesVanillaEngine(process, t_grid=400, x_grid=400)
    print(f"Percentage strike FD estimate: {option.price_fd(engine):.10f}")
    print(f"FD delta: {option.delta():.10f}")


if __name__ == "__main__":
    main()
