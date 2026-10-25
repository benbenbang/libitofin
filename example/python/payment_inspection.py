"""Inspect actual bond payments without installing a pricing engine."""
# itofin library
from itofin import Settings
from itofin.instruments import FixedRateBond
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Schedule

settings = Settings()
settings.set_evaluation_date(Date(20, 1, 2026))
calendar = Calendar.null_calendar()
schedule = Schedule(Date(15, 1, 2025), Date(15, 1, 2027), Frequency.Semiannual, calendar, BusinessDayConvention.Unadjusted)
bond = FixedRateBond(2, 1000.0, schedule, [.045], DayCounter.actual365_fixed(), settings)
print("Accrual starts:", bond.start_date())
print("Tradable:", bond.is_tradable())
print("Next payment:", bond.next_cash_flow_date(), bond.next_cash_flow_amount())
print("Final coupon plus principal:", bond.next_cash_flow_amount(Date(15, 7, 2026)))
leg = bond.cashflows()
print("Previous actual leg payment:", leg.previous_cash_flow_date(settings), leg.previous_cash_flow_amount(settings))
