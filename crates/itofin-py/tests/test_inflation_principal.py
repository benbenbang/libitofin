"""Standalone inflation principal against independent compiled QuantLib fixtures."""
import csv
import gc
import json
import math
from pathlib import Path

import pytest

from itofin import ItofinError, Settings
from itofin.cashflows import CPICashFlow, ZeroInflationCashFlow, npv
from itofin.indexes import CpiInterpolationType, ZeroInflationIndex
from itofin.termstructures import FlatForward, InterpolatedZeroInflationCurve
from itofin.time import Date, DayCounter, Frequency, Period

DATA = Path(__file__).resolve().parents[3] / "sdk/go/testdata"
INPUTS = json.loads((DATA / "inflation-principal-inputs.json").read_text())["inputs"]
with (DATA / "inflation-principal-native.tsv").open() as stream:
    CASES = list(csv.DictReader(stream, delimiter="\t"))
assert len(CASES) == 48


def date(iso):
    y, m, d = map(int, iso.split("-"))
    return Date(d, m, y)


def market():
    settings = Settings()
    settings.set_evaluation_date(date(INPUTS["evaluation_date"]))
    index = ZeroInflationIndex.uk_rpi(settings)
    for row in INPUTS["historical_fixings"]:
        index.add_fixing(date(row["date"]), row["value"])
    dc = DayCounter.actual365_fixed()
    curve = inflation_curve(dc, INPUTS["inflation_rate"])
    index.link_to(curve)
    return settings, index, dc, curve


def inflation_curve(dc, rate):
    return InterpolatedZeroInflationCurve(
        date(INPUTS["evaluation_date"]),
        [date(x) for x in INPUTS["inflation_curve_dates"]],
        [rate, rate], Frequency.Monthly, dc,
    )


def make_flow(row, index):
    common = (Period(int(row["lag_months"]), "Months"),
              CpiInterpolationType.Flat if row["interpolation"] == "flat" else CpiInterpolationType.Linear,
              date(row["payment_date"]), row["growth_only"] == "true")
    if row["kind"] == "explicit":
        return CPICashFlow(float(row["nominal"]), index, float(row["base_cpi"]),
                           date(row["observation_date"]), *common)
    return ZeroInflationCashFlow(float(row["nominal"]), index, date(row["start_date"]),
                                 date(row["observation_date"]), *common)


@pytest.mark.parametrize("row", CASES, ids=lambda row: row["id"])
def test_principal_native_original_live_relink_and_erased_lifetime(row):
    settings, index, dc, curve = market()
    flow = make_flow(row, index)
    generic = flow.as_cashflow()
    leg = flow.as_leg()
    assert flow.date() == date(row["payment_date"])
    assert generic.date() == flow.date()
    for name in ("amount", "base_fixing", "index_fixing"):
        tolerance = 1e-9 if name == "amount" else 1e-11
        assert getattr(flow, name)() == pytest.approx(float(row["original_" + name]), abs=tolerance, rel=0)
    assert generic.amount() == pytest.approx(flow.amount(), abs=1e-9, rel=0)
    assert len(leg) == 1
    assert leg[0].amount() == pytest.approx(flow.amount(), abs=1e-9, rel=0)
    replacement = inflation_curve(dc, INPUTS["relinked_inflation_rate"])
    index.link_to(replacement)
    for name in ("amount", "base_fixing", "index_fixing"):
        tolerance = 1e-9 if name == "amount" else 1e-11
        assert getattr(flow, name)() == pytest.approx(float(row["relinked_unmodified_" + name]), abs=tolerance, rel=0)
    expected = flow.amount()
    discount = FlatForward(date(INPUTS["evaluation_date"]), 0.035, dc)
    pay = date(row["payment_date"])
    days = pay - date(INPUTS["evaluation_date"])
    expected_npv = expected * math.exp(-0.035 * days / 365) if days > 0 else 0
    assert npv(leg, discount, settings, False) == pytest.approx(expected_npv, abs=1e-8, rel=0)
    other = Settings()
    other.set_evaluation_date(date(INPUTS["evaluation_date"]))
    with pytest.raises(ValueError, match="settings must match"):
        npv(leg, discount, other)
    del flow, index, curve, replacement
    gc.collect()
    assert generic.amount() == pytest.approx(expected, abs=1e-9, rel=0)
    assert leg[0].amount() == pytest.approx(expected, abs=1e-9, rel=0)


@pytest.mark.parametrize("lag", [Period(-1, "Months"), Period(2**31 - 1, "Months"), Period(2**31 - 1, "Days")])
def test_principal_bad_lag_is_recoverable(lag):
    _, index, _, _ = market()
    for make in (lambda: CPICashFlow(100, index, 200, Date(15, 7, 2024), lag, CpiInterpolationType.Flat, Date(15, 7, 2026)),
                 lambda: ZeroInflationCashFlow(100, index, Date(15, 1, 2024), Date(15, 7, 2024), lag, CpiInterpolationType.Flat, Date(15, 7, 2026))):
        with pytest.raises((ItofinError, ValueError)):
            make()


@pytest.mark.parametrize("base", [0, 1e-20, float("nan"), float("inf")])
def test_explicit_principal_rejects_invalid_base(base):
    _, index, _, _ = market()
    with pytest.raises((ItofinError, ValueError)):
        CPICashFlow(100, index, base, Date(15, 7, 2024), Period(3, "Months"), CpiInterpolationType.Flat, Date(15, 7, 2026))


def test_live_principal_missing_fixing_is_lazy_and_finite_checks_survive_erasure():
    settings = Settings()
    settings.set_evaluation_date(Date(18, 8, 2025))
    index = ZeroInflationIndex.uk_rpi(settings)
    flow = ZeroInflationCashFlow(100, index, Date(15, 1, 2024), Date(15, 7, 2024),
                                 Period(0, "Months"), CpiInterpolationType.Flat, Date(15, 7, 2026))
    generic, leg = flow.as_cashflow(), flow.as_leg()
    for query in (flow.amount, generic.amount, leg[0].amount):
        with pytest.raises(ItofinError):
            query()
    index.add_fixing(Date(1, 1, 2024), 0)
    index.add_fixing(Date(1, 7, 2024), 110)
    for query in (flow.amount, generic.amount, leg[0].amount):
        with pytest.raises(ItofinError, match="non-finite"):
            query()
    assert flow.base_fixing() == 0


@pytest.mark.parametrize("live", [False, True])
def test_principal_read_time_date_guards_survive_generic_conversion(live):
    settings, index, _, _ = market()
    common = (Period(3, "Months"), CpiInterpolationType.Linear, Date(31, 12, 2199))
    flow = (ZeroInflationCashFlow(100, index, Date(15, 1, 2024), Date(15, 12, 2199), *common)
            if live else CPICashFlow(100, index, 200, Date(15, 12, 2199), *common))
    for query in (flow.amount, flow.index_fixing, flow.as_cashflow().amount, flow.as_leg()[0].amount):
        with pytest.raises(ItofinError, match="outside date range"):
            query()
    normal = make_flow(CASES[0], index)
    erased = normal.as_cashflow()
    settings.set_evaluation_date(Date(1, 1, 1901))
    with pytest.raises(ItofinError, match="outside date range"):
        erased.amount()


def test_live_principal_rejects_overflow_base_even_when_final_ratio_would_be_finite():
    settings, index, dc, _ = market()
    curve = inflation_curve(dc, 1e308)
    index.link_to(curve)
    flow = ZeroInflationCashFlow(100, index, Date(15, 1, 2026), Date(15, 7, 2024),
                                 Period(0, "Months"), CpiInterpolationType.Flat, Date(15, 7, 2026))
    discount = FlatForward(date(INPUTS["evaluation_date"]), 0, dc)
    for query in (flow.base_fixing, flow.amount, flow.as_cashflow().amount,
                  flow.as_leg()[0].amount, lambda: npv(flow.as_leg(), discount, settings)):
        with pytest.raises(ItofinError, match="non-finite"):
            query()
    assert math.isfinite(flow.index_fixing())


@pytest.mark.parametrize("nominal", [float("nan"), float("inf"), float("-inf")])
def test_principal_rejects_nonfinite_nominal_in_both_constructors(nominal):
    _, index, _, _ = market()
    for make in (lambda: CPICashFlow(nominal, index, 200, Date(15, 7, 2024), Period(0, "Months"), CpiInterpolationType.Flat, Date(15, 7, 2026)),
                 lambda: ZeroInflationCashFlow(nominal, index, Date(15, 1, 2024), Date(15, 7, 2024), Period(0, "Months"), CpiInterpolationType.Flat, Date(15, 7, 2026))):
        with pytest.raises((ItofinError, ValueError)):
            make()
