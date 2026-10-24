"""Qualified independent QuantLib observations, including genuine query errors."""

import gc
import json
import math
from pathlib import Path

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import Currency, IborIndex
from itofin.instruments import AmortizingFloatingRateBond
from itofin.pricingengines import DiscountingBondEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import BondPriceType, FlatForward
from itofin.time import BusinessDayConvention as BDC, Calendar, Compounding, Date, DayCounter, Frequency, Period, Schedule

ROWS = [json.loads(line) for line in (Path(__file__).parent / "fixtures/amortizing_floating_oracle.jsonl").read_text().splitlines()]
FINANCIAL = [row for row in ROWS if "terms" in row]


def date(value):
    return Date(value[2], value[1], value[0])


def observe(read, expected):
    if not expected["ok"]:
        with pytest.raises(ItofinError):
            read()
        return None
    actual = read()
    value = expected["value"]
    assert math.isfinite(actual) and math.isfinite(value)
    assert abs(actual - value) <= 3e-11 + 3e-13 * abs(value)
    return actual


def graph(row):
    terms = row["terms"]
    settings = Settings()
    settings.set_evaluation_date(date(terms["evaluation_date"]))
    settings.set_using_at_par_coupons(terms["at_par"])
    settings.set_include_reference_date_events(False)
    settings.set_include_todays_cash_flows(False)
    calendar = Calendar.weekends_only() if terms["calendar"] == "WeekendsOnly" else Calendar.null_calendar()
    counter = DayCounter.actual360()
    discount_quote, forward_quote = SimpleQuote(terms["discount_quote"]), SimpleQuote(terms["forward_quote"])
    discount = FlatForward.from_quote(date(terms["discount_curve_reference"]), discount_quote, counter)
    forward = FlatForward.from_quote(date(terms["forward_curve_reference"]), forward_quote, counter)
    index = IborIndex("AmortizingQLFixture", Period(terms["index_tenor_months"], "Months"), terms["index_fixing_days"], Currency.usd(), calendar, BDC.Preceding, False, counter, forward, settings)
    for fixing in terms["fixings"]:
        index.add_fixing(date(fixing["date"]), fixing["value"])
    schedule = Schedule(date(terms["schedule_dates"][0]), date(terms["schedule_dates"][-1]), Frequency.Quarterly, calendar, BDC.Unadjusted)
    assert schedule.dates() == [date(value) for value in terms["schedule_dates"]]
    bond = AmortizingFloatingRateBond(terms["settlement_days"], terms["notionals"], schedule, index, counter, settings, fixing_days=terms["index_fixing_days"], spreads=terms["floating_spreads"], issue_date=date(terms["issue_date"]), ex_coupon_period=Period(terms["ex_coupon_days"], "Days") if terms["ex_coupon_days"] else None, ex_coupon_calendar=calendar)
    engine = DiscountingBondEngine(discount, settings)
    bond.set_engine(engine)
    return bond, settings, index, schedule, counter, discount, forward, discount_quote, forward_quote, engine


@pytest.mark.parametrize("row", FINANCIAL, ids=[f"{i}-{row['case_id']}-{row['phase']}" for i, row in enumerate(FINANCIAL)])
def test_amortizing_quantlib_financial_rows(row):
    owners = graph(row)
    bond, _, _, _, counter, *_ = owners
    assert bond.maturity_date() == date(row["maturity_date"])
    assert bond.settlement_date() == date(row["settlement_date"])
    assert bond.is_expired() == row["is_expired"]
    for name, read in [("npv", bond.npv), ("settlement_value", bond.settlement_value), ("clean_price", bond.clean_price), ("dirty_price", bond.dirty_price), ("accrued", bond.accrued_amount), ("notional", bond.notional)]:
        observe(read, row[name])
    leg, principal = bond.cashflows(), bond.redemptions()
    assert len(leg) == len(row["flows"])
    for i, expected in enumerate(row["flows"]):
        actual = leg[i]
        assert actual.date() == date(expected["payment_date"])
        observe(actual.amount, expected["amount"])
    assert len(principal) == len(row["redemptions"])
    for i, expected in enumerate(row["redemptions"]):
        actual = principal[i]
        assert actual.date() == date(expected["date"])
        observe(actual.amount, expected["amount"])
    for query in row["notionals_by_date"]:
        observe(lambda: bond.notional(date(query["date"])), query["value"])
    settlement = date(row["settlement_date"])
    for kind, price_type in [("clean", BondPriceType.Clean), ("dirty", BondPriceType.Dirty)]:
        key = kind + "_yield"
        if key not in row:
            continue
        if row["notional"]["ok"] and row["notional"]["value"] == 0.0:
            assert row[key]["ok"] and row[key]["value"] == 0.0
            assert row[key + "_inversion"]["ok"] == (kind == "dirty")
            if kind == "dirty":
                assert row[key + "_inversion"]["value"] == 0.0
            else:
                assert "non tradable" in row[key + "_inversion"]["message"]
            with pytest.raises(ItofinError, match="non tradable at .* settlement date"):
                bond.yield_rate(row[kind + "_price"]["value"], counter, Compounding.Compounded, Frequency.Semiannual, price_type=price_type, settlement=settlement, accuracy=1e-12, max_evaluations=200, guess=0.03)
            inverse = bond.clean_price_from_yield if kind == "clean" else bond.dirty_price_from_yield
            with pytest.raises(ItofinError, match="non tradable at .* settlement date"):
                inverse(row[key]["value"], counter, Compounding.Compounded, Frequency.Semiannual, settlement=settlement)
            continue
        rate = observe(lambda: bond.yield_rate(row[kind + "_price"]["value"], counter, Compounding.Compounded, Frequency.Semiannual, price_type=price_type, settlement=settlement, accuracy=1e-12, max_evaluations=200, guess=0.03), row[key])
        if rate is not None:
            read = bond.clean_price_from_yield if kind == "clean" else bond.dirty_price_from_yield
            read_rate = rate
            observe(lambda: read(read_rate, counter, Compounding.Compounded, Frequency.Semiannual, settlement=settlement), row[key + "_inversion"])


def test_amortizing_quantlib_retained_native_amounts():
    source = next(row for row in FINANCIAL if row["case_id"] == "retained_views_and_final_principal")
    expected = next(row for row in ROWS if row["phase"] == "retained_native_cashflows")
    owners = graph(source)
    leg = owners[0].cashflows()
    del owners
    gc.collect()
    assert len(leg) == len(expected["amounts"])
    for i, amount in enumerate(expected["amounts"]):
        observe(leg[i].amount, amount)
