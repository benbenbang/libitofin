"""Compare a supplied clean/dirty quote with the preserved own-NPV ATM target."""

# itofin library
from itofin import Settings
from itofin.instruments import FixedRateBond
from itofin.termstructures import BondPriceType, FlatForward
from itofin.time import BusinessDayConvention, Calendar, Date, DateGeneration, DayCounter, Frequency, Schedule

settings = Settings()
calendar, counter = Calendar.null_calendar(), DayCounter.actual360()
settlement = Date(26, 6, 2026)
schedule = Schedule(
    Date(24, 3, 2026),
    Date(24, 3, 2028),
    Frequency.Semiannual,
    calendar,
    BusinessDayConvention.Unadjusted,
    DateGeneration.Backward,
)
bond = FixedRateBond(5, 375.0, schedule, [0.047, 0.058], counter, settings)
curve = FlatForward(Date(1, 6, 2026), 0.032, DayCounter.actual365_fixed())
clean = 100.0
dirty = clean + bond.accrued_amount(settlement)
print("Clean price target ATM:", bond.atm_rate_from_price(curve, clean, settlement=settlement))
print(
    "Equivalent dirty target ATM:",
    bond.atm_rate_from_price(curve, dirty, price_type=BondPriceType.Dirty, settlement=settlement),
)
print("Preserved own-NPV ATM:", bond.atm_rate_from_curve(curve, settlement=settlement))
