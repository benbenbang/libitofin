"""Price a plain same-currency paid-3M/received-6M Ibor basis swap."""

# itofin library
from itofin import Settings
from itofin.indexes import Currency, IborIndex
from itofin.instruments import FloatFloatSwap
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention as BDC
from itofin.time import Calendar, Date, DayCounter, Frequency, Period, Schedule


def main():
    """Construct retained live dependencies and report signed PV and absolute fair spread."""
    settings = Settings()
    reference = Date(2, 1, 2026)
    settings.set_evaluation_date(reference)
    dc = DayCounter.actual360()
    curves = [FlatForward(reference, rate, dc) for rate in [0.025, 0.034, 0.02]]
    indexes = [
        IborIndex(
            f"ExampleFloat{i}",
            Period(months, "Months"),
            0,
            Currency.eur(),
            Calendar.null_calendar(),
            BDC.Unadjusted,
            False,
            dc,
            curves[i],
            settings,
        )
        for i, months in enumerate([3, 6])
    ]
    schedules = [
        Schedule(Date(17, 1, 2026), Date(17, 1, 2027), frequency, Calendar.weekends_only(), BDC.Unadjusted)
        for frequency in [Frequency.Quarterly, Frequency.Semiannual]
    ]
    swap = FloatFloatSwap(
        1_000_000.0,
        schedules[0],
        indexes[0],
        0.001,
        dc,
        schedules[1],
        indexes[1],
        -0.002,
        DayCounter.actual365_fixed(),
        settings,
    )
    swap.set_engine(curves[2], settings)
    print(f"NPV: {swap.npv():.10f}")
    print(f"absolute paid fair spread: {swap.fair_pay_spread():.10f}")


if __name__ == "__main__":
    main()
