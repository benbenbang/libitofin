"""QuantLib 1.43: final accrual 2026-06-20, Following payment 2026-06-22."""

import gc

from itofin import Settings
from itofin.instruments import CreditDefaultSwap, ProtectionSide
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Schedule


def test_protection_end_retains_unadjusted_coupon_after_inputs_are_collected():
    settings = Settings()
    settings.set_evaluation_date(Date(19, 6, 2025))
    calendar = Calendar.weekends_only()
    day_counter = DayCounter.actual360()
    schedule = Schedule(
        Date(20, 6, 2025),
        Date(20, 6, 2026),
        Frequency.Quarterly,
        calendar,
        BusinessDayConvention.Following,
        termination_convention=BusinessDayConvention.Unadjusted,
    )
    cds = CreditDefaultSwap(
        ProtectionSide.Buyer,
        10_000_000.0,
        0.01,
        schedule,
        BusinessDayConvention.Following,
        day_counter,
        True,
        True,
        settings,
    )
    end = cds.protection_end_date()
    assert end == Date(20, 6, 2026)
    assert calendar.adjust(end, BusinessDayConvention.Following) == Date(22, 6, 2026)
    settings.set_evaluation_date(Date(23, 6, 2026))
    assert cds.protection_end_date() == end
    del settings, calendar, day_counter, schedule
    gc.collect()
    assert cds.protection_end_date() == end
    del cds
    gc.collect()
    assert end == Date(20, 6, 2026)
