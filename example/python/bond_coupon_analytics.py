"""Price-independent coupon BPS per100 and own-NPV ATM rate."""
# itofin library
from itofin import Settings
from itofin.instruments import FixedRateBond
from itofin.termstructures import FlatForward
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Compounding,
    Date,
    DateGeneration,
    DayCounter,
    Frequency,
    Schedule,
)

settings = Settings()
settlement = Date(26, 6, 2026)
calendar, counter = Calendar.null_calendar(), DayCounter.actual360()
schedule = Schedule(Date(24, 3, 2026), Date(24, 3, 2028), Frequency.Semiannual,
                    calendar, BusinessDayConvention.Unadjusted, DateGeneration.Backward)
bond = FixedRateBond(5, 375.0, schedule, [.047, .058], counter, settings)
curve = FlatForward(Date(1, 6, 2026), .032, DayCounter.actual365_fixed())
print("Coupon BPS per100:", bond.bps_from_curve(curve, settlement=settlement))
print("Yield-discounted coupon BPS:", bond.bps_from_yield(.052, counter, Compounding.Continuous,
                                                       Frequency.Annual, settlement=settlement))
print("Own-NPV ATM coupon rate:", bond.atm_rate_from_curve(curve, settlement=settlement))
