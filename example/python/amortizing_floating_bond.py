"""Plain quarterly Ibor coupons on declining balances, with live forwarding."""

# standard library
import math

# itofin library
from itofin import Settings
from itofin.indexes import Currency, IborIndex
from itofin.instruments import AmortizingFloatingRateBond
from itofin.pricingengines import DiscountingBondEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention as BDC
from itofin.time import Calendar, Date, DayCounter, Frequency, Period, Schedule

settings = Settings()
today = Date(28, 9, 2026)
settings.set_evaluation_date(today)
calendar, counter = Calendar.null_calendar(), DayCounter.actual360()
rate = SimpleQuote(0.04)
forward = FlatForward.from_quote(today, rate, counter)
discount = FlatForward(today, 0.025, counter)
index = IborIndex("AmortizingExample", Period(6, "Months"), 2, Currency.eur(), calendar, BDC.Following, False, counter, forward, settings)
schedule = Schedule(Date(5, 10, 2026), Date(5, 10, 2027), Frequency.Quarterly, calendar, BDC.Unadjusted)
bond = AmortizingFloatingRateBond(0, [1000.0, 800.0, 800.0, 500.0], schedule, index, counter, settings, spreads=[0.001])
bond.set_engine(DiscountingBondEngine(discount, settings))
initial = bond.npv()
rate.set_value(0.05)
updated = bond.npv()
assert math.isfinite(initial) and math.isfinite(updated) and updated > initial
assert [payment.amount() for payment in bond.redemptions()] == [200.0, 300.0, 500.0]
print(f"NPV before/after forwarding update: {initial:.10f} / {updated:.10f}")
print("Principal:", [(flow.date(), flow.amount()) for flow in bond.redemptions()])
