"""Lazy base-date CPI coupon and leg with historical observations."""
# itofin library
from itofin import Settings
from itofin.cashflows import CPICoupon, CPICouponPricer, CPILeg, npv
from itofin.indexes import CpiInterpolationType, ZeroInflationIndex
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Period, Schedule

settings = Settings()
settings.set_evaluation_date(Date(15, 6, 2025))
index = ZeroInflationIndex.uk_rpi(settings)
index.add_fixing(Date(1, 10, 2024), 99.5)
index.add_fixing(Date(1, 4, 2025), 100.8)
dc = DayCounter.actual365_fixed()
lag = Period(3, "Months")
coupon = CPICoupon.from_base_date(Date(15, 10, 2024), Date(15, 7, 2025), 100000.0,
                                 Date(15, 1, 2025), Date(15, 7, 2025), index, lag,
                                 CpiInterpolationType.Flat, dc, 0.02)
coupon.set_pricer(CPICouponPricer())
print("Effective base CPI", coupon.base_fixing())
print("Coupon amount", coupon.amount())
calendar = Calendar.null_calendar()
schedule = Schedule(Date(15, 1, 2025), Date(15, 7, 2025), Frequency.Semiannual,
                    calendar, BusinessDayConvention.Unadjusted)
leg = CPILeg.from_base_date(schedule, index, lag, [100000.0], [0.02], dc, calendar,
                            BusinessDayConvention.Unadjusted, growth_only=False).build()
discount = FlatForward(Date(15, 6, 2025), 0.04, dc)
print("NPV", npv(leg, discount, settings))
