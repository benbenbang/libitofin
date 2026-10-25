"""Payment navigation is not NPV: selected actual flows include ex-coupon payments."""
import gc

import pytest

from itofin import ItofinError, Settings
from itofin.cashflows import CPILeg
from itofin.indexes import CpiInterpolationType
from itofin.time import BusinessDayConvention as BDC, Calendar, Date, Frequency, Period, Schedule
from test_bond_analytics_bindings import floating_market
from test_bond_coupon_analytics_bindings import market
from test_cpi_cashflows import setup as cpi_setup


@pytest.mark.parametrize("face", [100.0, 375.0, 750.0])
def test_bond_payment_inspection_actual_units_and_unpriced_state(face):
    bond, settings, dc, curve, settlement, dates = market(face)
    leg = bond.cashflows()
    assert bond.start_date() == leg.start_date() == dates[0]
    assert leg.maturity_date() == bond.maturity_date() == dates[-1]
    assert bond.is_tradable(settlement)
    assert not bond.is_tradable(dates[-1])
    assert bond.previous_cash_flow_date(settlement) is None
    assert bond.previous_cash_flow_amount(settlement) is None
    assert bond.next_cash_flow_date(settlement) == dates[1]
    assert bond.next_cash_flow_amount(settlement) == pytest.approx(face * .047 * (dates[1] - dates[0]) / 360, abs=3e-12, rel=3e-13)
    final_amount = face * (.058 * (dates[-1] - dates[-2]) / 360 + 1.02)
    assert bond.next_cash_flow_date(dates[-2]) == dates[-1]
    assert bond.next_cash_flow_amount(dates[-2]) == pytest.approx(final_amount, abs=3e-12, rel=3e-13)
    assert bond.previous_cash_flow_date(dates[-1]) == dates[-1]
    assert bond.previous_cash_flow_amount(dates[-1]) == pytest.approx(final_amount, abs=3e-12, rel=3e-13)
    assert bond.next_cash_flow_date(dates[-1]) is None
    assert bond.next_cash_flow_amount(dates[-1]) is None
    assert not bond.is_calculated()
    with pytest.raises(ItofinError):
        bond.next_cash_flow_date()
    settings.set_evaluation_date(Date(19, 6, 2026))
    assert bond.next_cash_flow_date() == bond.next_cash_flow_date(bond.settlement_date())
    assert bond.next_cash_flow_amount() == bond.next_cash_flow_amount(bond.settlement_date())
    del bond, settings, dc, curve
    gc.collect()
    assert leg.previous_cash_flow_amount(Settings(), settlement_date=dates[-1]) == pytest.approx(final_amount, abs=3e-12, rel=3e-13)


@pytest.mark.parametrize("include_today", [None, False, True])
@pytest.mark.parametrize("include_settlement", [None, False, True])
def test_leg_payment_inspection_today_override_and_optional_policy(include_today, include_settlement):
    bond, settings, dc, curve, settlement, dates = market()
    leg = bond.cashflows()
    settings.set_evaluation_date(dates[1])
    settings.set_include_todays_cash_flows(include_today)
    options = {"include_settlement_date_flows": include_settlement, "settlement_date": dates[1]}
    includes = include_today if include_today is not None else bool(include_settlement)
    assert leg.next_cash_flow_date(settings, **options) == dates[1 if includes else 2]
    assert leg.previous_cash_flow_date(settings, **options) == (None if includes else dates[1])
    if includes:
        assert leg.previous_cash_flow_amount(settings, **options) is None
    else:
        assert leg.previous_cash_flow_amount(settings, **options) == leg[0].amount()
    assert leg.next_cash_flow_amount(settings, **options) == leg[0 if includes else 1].amount()
    assert leg.next_cash_flow_date(settings, include_settlement_date_flows=include_settlement) == dates[1 if includes else 2]


def test_date_only_does_not_require_floating_fixings_and_selected_amount_does():
    bond, settings, dc, curve, forward, vol = floating_market()
    leg = bond.cashflows()
    settlement = Date(26, 6, 2026)
    bond.freeze()
    settings.set_evaluation_date(Date(24, 12, 2026))
    assert bond.start_date() == leg.start_date()
    assert bond.is_tradable(settlement)
    assert bond.next_cash_flow_date(settlement) is not None
    assert leg.next_cash_flow_date(settings, settlement_date=settlement) is not None
    with pytest.raises(ItofinError):
        bond.next_cash_flow_amount(settlement)
    with pytest.raises(ItofinError):
        leg.next_cash_flow_amount(settings, settlement_date=settlement)
    later = bond.next_cash_flow_amount(Date(24, 3, 2027))
    assert later is not None and later > 0.0
    principal = bond.redemptions()
    del bond
    gc.collect()
    amount = principal.next_cash_flow_amount(settings, settlement_date=settlement)
    assert amount is not None and amount > 0.0


def test_cpi_payment_inspection_settings_and_released_graph():
    settings, index, dc, discount = cpi_setup()
    cal = Calendar.null_calendar()
    schedule = Schedule(Date(15, 1, 2025), Date(15, 7, 2025), Frequency.Semiannual, cal, BDC.Unadjusted)
    builder = CPILeg(schedule, index, 100.0, Period(3, "Months"), [100000.0], [.02], dc, cal, BDC.Unadjusted, CpiInterpolationType.Flat, False)
    leg = builder.build()
    other = Settings()
    other.set_evaluation_date(Date(15, 6, 2025))
    for name in ["previous_cash_flow_date", "next_cash_flow_date", "previous_cash_flow_amount", "next_cash_flow_amount"]:
        with pytest.raises(ValueError, match="settings must match"):
            getattr(leg, name)(other)
    expected = sum(leg[i].amount() for i in range(len(leg)))
    del index, builder, schedule, cal, dc, discount
    gc.collect()
    assert leg.next_cash_flow_date(settings) == Date(15, 7, 2025)
    assert leg.next_cash_flow_amount(settings) == pytest.approx(expected, abs=1e-9, rel=0)


def test_invalid_date_types_fail_without_silent_default():
    bond, settings, dc, curve, settlement, dates = market()
    leg = bond.cashflows()
    for name in ["is_tradable", "previous_cash_flow_date", "next_cash_flow_date", "previous_cash_flow_amount", "next_cash_flow_amount"]:
        with pytest.raises(TypeError):
            getattr(bond, name)(0)
    for name in ["previous_cash_flow_date", "next_cash_flow_date", "previous_cash_flow_amount", "next_cash_flow_amount"]:
        with pytest.raises(TypeError):
            getattr(leg, name)(settings, settlement_date=0)


def test_payment_inspection_independent_native_matrix():
    import csv
    import json
    from pathlib import Path

    from itofin.instruments import FixedRateBond
    from itofin.time import DateGeneration, DayCounter

    root = Path(__file__).resolve().parents[3] / "sdk/go/testdata/payment_inspection"
    fixture = json.loads((root / "bond-native.json").read_text())

    def date(iso):
        year, month, day = map(int, iso.split("-"))
        return Date(day, month, year)

    inputs = fixture["inputs"]
    settings = Settings()
    settings.set_evaluation_date(date(inputs["evaluation_date"]))
    dc, calendar = DayCounter.actual360(), Calendar.null_calendar()
    schedule = Schedule(date(inputs["start"]), date(inputs["end"]), Frequency.Semiannual, calendar, BDC.Unadjusted, DateGeneration.Forward)
    bond = FixedRateBond(inputs["settlement_days"], inputs["nominal"], schedule, inputs["rates"], dc, settings,
                         payment_convention=BDC.Unadjusted, redemption=inputs["redemption"], issue_date=date(inputs["start"]),
                         ex_coupon_period=Period(7, "Days"), ex_coupon_calendar=calendar, ex_coupon_convention=BDC.Unadjusted)
    leg = bond.cashflows()
    assert bond.start_date() == leg.start_date() == date(fixture["start_date"])
    assert bond.maturity_date() == leg.maturity_date() == date(fixture["maturity_date"])
    assert bond.settlement_date() == date(fixture["settlement_date"])
    rows = list(csv.DictReader((root / "bond-native.tsv").open(), delimiter="\t"))
    assert len(rows) == fixture["rows"] == 60
    for row in rows:
        settings.set_include_todays_cash_flows(None if row["include_today"] == "unset" else row["include_today"] == "true")
        settlement = date(row["settlement"])
        options = {"settlement_date": settlement, "include_settlement_date_flows": row["include_settlement"] == "true"}
        assert bond.is_tradable(settlement) == (row["tradable"] == "true")
        for direction in ["previous", "next"]:
            actual_date = getattr(leg, direction + "_cash_flow_date")(settings, **options)
            actual_amount = getattr(leg, direction + "_cash_flow_amount")(settings, **options)
            wanted_date, wanted_amount = row[direction + "_date"], row[direction + "_amount"]
            assert actual_date == (None if wanted_date == "-" else date(wanted_date))
            if wanted_amount == "-":
                assert actual_amount is None
            else:
                assert actual_amount == pytest.approx(float(wanted_amount), abs=3e-12, rel=3e-13)
            if row["include_settlement"] == "false":
                assert getattr(bond, direction + "_cash_flow_date")(settlement) == actual_date
                amount = getattr(bond, direction + "_cash_flow_amount")(settlement)
                assert amount is None if actual_amount is None else amount == pytest.approx(actual_amount, abs=3e-12, rel=3e-13)


def test_missing_cpi_coupon_does_not_block_unrelated_selected_date_or_principal():
    from test_cpi_bond import make

    bond, index, settings, quote = make(omit_apr=True)
    leg, principal = bond.cashflows(), bond.redemptions()
    settlement = Date(15, 6, 2025)
    bond.freeze()
    assert bond.next_cash_flow_date(settlement) == Date(15, 7, 2025)
    assert leg.next_cash_flow_date(settings, settlement_date=settlement) == Date(15, 7, 2025)
    with pytest.raises(ItofinError):
        bond.next_cash_flow_amount(settlement)
    with pytest.raises(ItofinError):
        leg.next_cash_flow_amount(settings, settlement_date=settlement)
    later = bond.next_cash_flow_amount(Date(15, 7, 2025))
    assert later is not None and later > 0.0
    del bond, index, quote
    gc.collect()
    amount = principal.next_cash_flow_amount(settings, settlement_date=settlement)
    assert amount is not None and amount > 0.0
