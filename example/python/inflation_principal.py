"""Compare fixed and live CPI denominators and value their one-flow legs."""
# itofin library
from itofin import Settings
from itofin.cashflows import CPICashFlow, ZeroInflationCashFlow, npv
from itofin.indexes import CpiInterpolationType, ZeroInflationIndex
from itofin.termstructures import FlatForward
from itofin.time import Date, DayCounter, Period

settings = Settings()
settings.set_evaluation_date(Date(18, 8, 2025))
index = ZeroInflationIndex.uk_rpi(settings)
index.add_fixing(Date(1, 1, 2024), 90.0)
index.add_fixing(Date(1, 7, 2024), 110.0)
lag = Period(0, "Months")
interpolation = CpiInterpolationType.Flat
payment = Date(15, 7, 2026)
fixed = CPICashFlow(1000, index, 100, Date(15, 7, 2024), lag, interpolation, payment)
live = ZeroInflationCashFlow(1000, index, Date(15, 1, 2024), Date(15, 7, 2024), lag, interpolation, payment)
discount = FlatForward(Date(18, 8, 2025), 0.04, DayCounter.actual365_fixed())
for label, flow in (("explicit base", fixed), ("live base", live)):
    print(label, "amount", flow.amount(), "NPV", npv(flow.as_leg(), discount, settings))
