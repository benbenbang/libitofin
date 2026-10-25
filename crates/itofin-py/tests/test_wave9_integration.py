"""Cross-feature retained inflation graphs and payment inspection."""
import gc

import pytest

from itofin import ItofinError, Settings
from itofin.cashflows import CPICashFlow, CPILeg, ZeroInflationCashFlow, npv
from itofin.indexes import CpiInterpolationType, ZeroInflationIndex
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Period, Schedule


@pytest.mark.parametrize("live_base", [False, True])
def test_wave9_principal_inspection_preserves_lazy_guards_and_retention(live_base: bool):
    settings = Settings()
    today, payment = Date(1, 1, 2025), Date(15, 7, 2026)
    settings.set_evaluation_date(today)
    index = ZeroInflationIndex.uk_rpi(settings)
    lag = Period(0, "Months")
    kind = CpiInterpolationType.Flat
    flow = (ZeroInflationCashFlow(1000, index, Date(15, 1, 2024), Date(15, 7, 2024), lag, kind, payment)
            if live_base else CPICashFlow(1000, index, 100, Date(15, 7, 2024), lag, kind, payment))
    generic, leg = flow.as_cashflow(), flow.as_leg()
    assert leg.start_date() == leg.maturity_date() == payment
    assert leg.next_cash_flow_date(settings) == payment
    assert leg.previous_cash_flow_date(settings) is None
    assert leg.previous_cash_flow_amount(settings) is None
    with pytest.raises(ItofinError):
        leg.next_cash_flow_amount(settings)
    index.add_fixing(Date(1, 1, 2024), 100)
    index.add_fixing(Date(1, 7, 2024), 120)
    assert leg.next_cash_flow_amount(settings) == pytest.approx(200, abs=1e-10, rel=0)
    other = Settings()
    other.set_evaluation_date(today)
    with pytest.raises(ValueError, match="settings must match"):
        leg.next_cash_flow_date(other)
    with pytest.raises(ValueError, match="settings must match"):
        leg.next_cash_flow_amount(other)
    settings.set_evaluation_date(Date(1, 1, 1901))
    assert leg.next_cash_flow_date(settings) == payment
    with pytest.raises(ItofinError, match="outside date range"):
        leg.next_cash_flow_amount(settings)
    with pytest.raises(ItofinError, match="outside date range"):
        generic.amount()
    settings.set_evaluation_date(today)
    del flow, index
    gc.collect()
    assert leg.next_cash_flow_amount(settings) == pytest.approx(200, abs=1e-10, rel=0)
    assert generic.amount() == pytest.approx(200, abs=1e-10, rel=0)
    discount = FlatForward(today, 0, DayCounter.actual365_fixed())
    assert npv(leg, discount, settings) == pytest.approx(200, abs=1e-10, rel=0)
    with pytest.raises(ValueError, match="settings must match"):
        npv(leg, discount, other)
    after_payment = Date(16, 7, 2026)
    assert leg.next_cash_flow_date(settings, settlement_date=after_payment) is None
    assert leg.next_cash_flow_amount(settings, settlement_date=after_payment) is None
    assert leg.previous_cash_flow_amount(settings, settlement_date=after_payment) == pytest.approx(200, abs=1e-10, rel=0)


def test_wave9_base_date_leg_inspection_keeps_principal_lazy_and_aggregated():
    settings = Settings()
    settlement, payment = Date(1, 1, 2024), Date(31, 7, 2024)
    settings.set_evaluation_date(settlement)
    index = ZeroInflationIndex.uk_rpi(settings)
    dc, calendar = DayCounter.actual365_fixed(), Calendar.null_calendar()
    schedule = Schedule(Date(31, 1, 2024), payment, Frequency.Semiannual, calendar, BusinessDayConvention.Unadjusted)
    builder = CPILeg.from_base_date(schedule, index, Period(1, "Months"), [1000.0], [0.0], dc, calendar, BusinessDayConvention.Unadjusted)
    leg = builder.build()
    assert len(leg) == 2
    assert leg[0].amount() == 0
    assert leg.next_cash_flow_date(settings) == payment
    with pytest.raises(ItofinError):
        leg.next_cash_flow_amount(settings)
    settings.set_evaluation_date(Date(1, 1, 2025))
    index.add_fixing(Date(1, 12, 2023), 100)
    index.add_fixing(Date(1, 6, 2024), 120)
    del builder, index, schedule, calendar
    gc.collect()
    assert leg.next_cash_flow_amount(settings, settlement_date=settlement) == pytest.approx(200, abs=1e-10, rel=0)
    assert leg.next_cash_flow_amount(settings, settlement_date=payment, include_settlement_date_flows=False) is None
    assert leg.previous_cash_flow_amount(settings, settlement_date=payment, include_settlement_date_flows=False) == pytest.approx(200, abs=1e-10, rel=0)
    assert leg.next_cash_flow_amount(settings, settlement_date=payment, include_settlement_date_flows=True) == pytest.approx(200, abs=1e-10, rel=0)
    discount = FlatForward(settlement, 0, dc)
    assert npv(leg, discount, settings, False, settlement, settlement) == pytest.approx(200, abs=1e-10, rel=0)
