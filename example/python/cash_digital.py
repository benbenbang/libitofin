"""European fixed-cash digital option on the existing analytic BSM engine."""

# itofin library
from itofin import Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.processes import BlackScholesProcess
from itofin.time import Date, DayCounter


def main():
    reference = Date(15, 6, 2026)
    settings = Settings()
    settings.set_evaluation_date(reference)
    process = BlackScholesProcess(
        100.0, 0.05, 0.02, 0.2, reference, DayCounter.actual360()
    )
    option = VanillaOption.cash_or_nothing(
        OptionType.Call, 105.0, 12.0, reference + 360, settings
    )
    print(f"Digital NPV: {option.price(process):.10f}")
    print(f"Digital delta: {option.delta():.10f}")


if __name__ == "__main__":
    main()
