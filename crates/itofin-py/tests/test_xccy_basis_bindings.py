"""Compiled QuantLib 1.43 cross-currency fixtures and public ownership contracts."""

# standard library
import csv
import gc
import math
from pathlib import Path
from typing import TypedDict

# pypi/conda library
import pytest

# itofin library
from itofin import ItofinError, Settings
from itofin.indexes import Currency, IborIndex
from itofin.instruments import XccyBasisSwap
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention as BDC
from itofin.time import Calendar, Date, DayCounter, Frequency, Period, Schedule

ROWS = list(csv.DictReader((Path(__file__).parent / "fixtures/xccy_basis_native.csv").read_text().splitlines()))


class OptionalArgs(TypedDict, total=False):
    """Optional fixed valuation terms."""

    settlement_date: Date
    npv_date: Date
    include_settlement_flows: bool
    spot_settlement_date: Date


class Args(OptionalArgs):
    """Exact additive constructor contract."""

    base_schedule: Schedule
    base_index: IborIndex
    base_curve: FlatForward
    base_notional: float
    base_day_count: DayCounter
    quote_schedule: Schedule
    quote_index: IborIndex
    quote_curve: FlatForward
    quote_notional: float
    quote_day_count: DayCounter
    quote_spread: float
    base_currency: Currency
    quote_currency: Currency
    spot: float
    settings: Settings


def date(serial: str | int) -> Date:
    """Convert the independently generated serial without a production date parser."""
    return Date(2, 1, 2026) + (int(serial) - 46024)


def market(row: dict[str, str]) -> tuple[Args, list[SimpleQuote]]:
    """Rebuild the exact native constructor profile."""
    settings = Settings()
    settings.set_evaluation_date(date(row["evaluation"]))
    override = int(row["today_override"])
    settings.set_include_todays_cash_flows(None if override < 0 else bool(override))
    quotes = [
        SimpleQuote(float(row[key])) for key in ["base_forward", "quote_forward", "base_discount", "quote_discount"]
    ]
    dc = DayCounter.actual360()
    curves = [FlatForward.from_quote(date(46027), quote, dc) for quote in quotes]
    currencies = [Currency.eur(), Currency.usd()]
    indexes = [
        IborIndex(
            f"xccy-{i}",
            Period(3, "Months"),
            0,
            currencies[i],
            Calendar.null_calendar(),
            BDC.Unadjusted,
            False,
            dc,
            curves[i],
            settings,
        )
        for i in range(2)
    ]
    bs = Schedule(Date(8, 1, 2026), Date(8, 1, 2027), Frequency.Quarterly, Calendar.null_calendar(), BDC.Unadjusted)
    qs, qe, calendar = Date(8, 1, 2026), Date(8, 1, 2027), Calendar.null_calendar()
    if row["asymmetric"] == "1":
        qs, qe = Date(8, 2, 2026), Date(8, 5, 2027)
    elif row["asymmetric"] == "2":
        qs, qe, calendar = Date(17, 1, 2026), Date(17, 1, 2027), Calendar.weekends_only()
    quote_schedule = Schedule(qs, qe, Frequency.Quarterly, calendar, BDC.Unadjusted)
    for i, schedule in enumerate([bs, quote_schedule]):
        for fixing in schedule.dates()[:-1]:
            if (fixing - date(row["evaluation"])) < 0:
                indexes[i].add_fixing(fixing, 0.022 if i == 0 else 0.034)
    args: Args = Args(
        base_schedule=bs,
        base_index=indexes[0],
        base_curve=curves[2],
        base_notional=1_000_000.0,
        base_day_count=dc,
        quote_schedule=quote_schedule,
        quote_index=indexes[1],
        quote_curve=curves[3],
        quote_notional=float(row["quote_notional"]),
        quote_day_count=dc,
        quote_spread=float(row["spread"]),
        base_currency=currencies[0],
        quote_currency=currencies[1],
        spot=1.27,
        settings=settings,
        settlement_date=date(row["cutoff"]),
        npv_date=date(row["npv_date"]),
        include_settlement_flows=bool(int(row["include"])),
        spot_settlement_date=date(row["spot_settlement"]),
    )
    return args, quotes


@pytest.mark.parametrize("row", ROWS, ids=lambda row: row["name"])
def test_xccy_native_oracle(row):
    """PV/BPS absolute tolerance2e-9 and fair spread2e-13 match retained core fixtures."""
    args, _ = market(row)
    swap = XccyBasisSwap(**args)
    for method, column in [
        (swap.base_leg_pv, "base_pv"),
        (swap.quote_leg_pv, "quote_pv"),
        (swap.quote_leg_bps, "quote_bps"),
        (swap.npv, "npv_base"),
        (swap.npv_in_quote, "npv_quote"),
    ]:
        assert method() == pytest.approx(float(row[column]), abs=2e-9, rel=0)
    assert swap.spot() == 1.27
    assert swap.base_currency().code() == "EUR"
    assert swap.quote_currency().code() == "USD"
    if row["name"] == "expired":
        with pytest.raises(ItofinError):
            swap.fair_quote_basis_spread()
    else:
        assert swap.fair_quote_basis_spread() == pytest.approx(float(row["fair_spread"]), abs=2e-13, rel=0)
        args["quote_spread"] = swap.fair_quote_basis_spread()
        assert XccyBasisSwap(**args).npv_in_quote() == pytest.approx(0, abs=2e-9, rel=0)


def test_xccy_retention_and_live_inputs():
    """Deleting wrappers keeps exact quote graphs and currency snapshots alive."""
    args, quotes = market(ROWS[0])
    swap = XccyBasisSwap(**args)
    before = swap.npv_in_quote()
    currency = swap.base_currency()
    del args
    gc.collect()
    for quote, rate in zip(quotes, [0.052, 0.075, 0.05, 0.06]):
        quote.set_value(rate)
    assert swap.npv_in_quote() == pytest.approx(float(ROWS[7]["npv_quote"]), abs=2e-9, rel=0)
    assert swap.npv_in_quote() != before
    assert swap.spot() == 1.27
    del swap
    gc.collect()
    assert currency.code() == "EUR"


def test_xccy_leg_selective_fixings_and_default_dates():
    """A base-leg query does not demand missing quote-leg fixings."""
    args, _ = market(ROWS[0])
    args.pop("settlement_date")
    args.pop("npv_date")
    args.pop("include_settlement_flows")
    args.pop("spot_settlement_date")
    swap = XccyBasisSwap(**args)
    args["base_index"].add_fixing(Date(8, 1, 2026), 0.022)
    args["settings"].set_evaluation_date(Date(9, 1, 2026))
    assert math.isfinite(swap.base_leg_pv())
    for method in [swap.quote_leg_pv, swap.quote_leg_bps, swap.npv, swap.npv_in_quote, swap.fair_quote_basis_spread]:
        with pytest.raises(ItofinError):
            method()
    assert swap.spot() == 1.27
    args["quote_index"].add_fixing(Date(8, 1, 2026), 0.034)
    assert math.isfinite(swap.npv_in_quote())


@pytest.mark.parametrize(
    "field,value",
    [
        ("spot", math.nan),
        ("spot", 0.0),
        ("base_notional", -1.0),
        ("quote_notional", math.inf),
        ("quote_spread", math.nan),
    ],
)
def test_xccy_invalid_numeric_fields(field, value):
    """Invalid domain values fail without replacing existing constructor signatures."""
    args, _ = market(ROWS[0])
    if field == "spot":
        args["spot"] = value
    elif field == "base_notional":
        args["base_notional"] = value
    elif field == "quote_notional":
        args["quote_notional"] = value
    else:
        args["quote_spread"] = value
    with pytest.raises(ItofinError):
        XccyBasisSwap(**args)


def test_xccy_currency_settings_and_dates_errors():
    """Core identity and discount reference checks remain enforced."""
    args, _ = market(ROWS[0])
    args["quote_currency"] = Currency.eur()
    with pytest.raises(ItofinError):
        XccyBasisSwap(**args)
    args, _ = market(ROWS[0])
    args["settings"] = Settings()
    with pytest.raises(ItofinError):
        XccyBasisSwap(**args)
    args, _ = market(ROWS[0])
    args["quote_curve"] = FlatForward(Date(6, 1, 2026), 0.04, DayCounter.actual360())
    with pytest.raises(ItofinError):
        XccyBasisSwap(**args).npv()
    args, _ = market(ROWS[0])
    args.pop("npv_date")
    with pytest.raises(ItofinError):
        XccyBasisSwap(**args)
