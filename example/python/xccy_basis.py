"""Plain constant-notional Ibor cross-currency swap with snapshot spot."""

# itofin library
from itofin import Settings
from itofin.indexes import Currency, IborIndex
from itofin.instruments import XccyBasisSwap
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention as BDC
from itofin.time import Calendar, Date, DayCounter, Frequency, Period, Schedule

settings = Settings()
settings.set_evaluation_date(Date(5, 1, 2026))
dc = DayCounter.actual360()
calendar = Calendar.null_calendar()
euro, dollar = Currency.eur(), Currency.usd()
base_curve = FlatForward(Date(5, 1, 2026), 0.03, dc)
quote_curve = FlatForward(Date(5, 1, 2026), 0.04, dc)
base_index = IborIndex(
    "example-EUR", Period(3, "Months"), 0, euro, calendar, BDC.Unadjusted, False, dc, base_curve, settings
)
quote_index = IborIndex(
    "example-USD", Period(3, "Months"), 0, dollar, calendar, BDC.Unadjusted, False, dc, quote_curve, settings
)
schedule = Schedule(Date(8, 1, 2026), Date(8, 1, 2027), Frequency.Quarterly, calendar, BDC.Unadjusted)
swap = XccyBasisSwap(
    schedule,
    base_index,
    base_curve,
    1_000_000.0,
    dc,
    schedule,
    quote_index,
    quote_curve,
    1_270_000.0,
    dc,
    0.0017,
    euro,
    dollar,
    1.27,
    settings,
)
print("EUR NPV", swap.npv())
print("USD NPV", swap.npv_in_quote())
print("Absolute fair quote spread", swap.fair_quote_basis_spread())
