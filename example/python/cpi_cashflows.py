"""Price a plain explicit-base CPI leg with an indexed terminal payment."""
# itofin library
from itofin import Settings
from itofin.cashflows import CPILeg, npv
from itofin.indexes import CpiInterpolationType, ZeroInflationIndex
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Period, Schedule

settings = Settings()
today = Date(15, 6, 2025)
settings.set_evaluation_date(today)
index = ZeroInflationIndex.uk_rpi(settings)
index.add_fixing(Date(1, 4, 2025), 100.8)
day_counter = DayCounter.actual365_fixed()
calendar = Calendar.null_calendar()
schedule = Schedule(
    Date(15, 1, 2025), Date(15, 7, 2025), Frequency.Semiannual,
    calendar, BusinessDayConvention.Unadjusted,
)
builder = CPILeg(
    schedule, index, 100.0, Period(3, "Months"), [100000.0], [0.02],
    day_counter, calendar, BusinessDayConvention.Unadjusted,
    CpiInterpolationType.Flat, False,
)
leg = builder.build()
discount = FlatForward(today, 0.04, day_counter)
for i in range(len(leg)):
    print(f"flow{i}: {leg[i].amount():.8f}")
print(f"NPV: {npv(leg, discount, settings):.8f}")
