"""Inspect par principal reductions and current-notional bond quotes."""

# itofin library
from itofin import Settings
from itofin.instruments import AmortizingFixedRateBond
from itofin.pricingengines import DiscountingBondEngine
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Schedule

settings = Settings()
settings.set_evaluation_date(Date(20, 1, 2026))
counter = DayCounter.actual360()
schedule = Schedule(
    Date(17, 1, 2024), Date(17, 1, 2027), Frequency.Annual, Calendar.weekends_only(), BusinessDayConvention.Unadjusted
)
bond = AmortizingFixedRateBond(2, [1000.0, 750.0, 400.0], schedule, [0.045, 0.052, 0.039], counter, settings)
curve = FlatForward(Date(20, 1, 2026), 0.031, counter)
bond.set_engine(DiscountingBondEngine(curve, settings))
print("Outstanding principal:", bond.notional())
print("Dirty price per100:", bond.dirty_price())
print("Currency settlement value:", bond.settlement_value())
principal_leg = bond.redemptions()
for index in range(len(principal_leg)):
    principal = principal_leg[index]
    print("Principal:", principal.date(), principal.amount())
