"""Bounded immutable CPI price snapshots against a mixed native QuantLib oracle."""

import csv
import gc
import hashlib
import json
import math
from pathlib import Path
from typing import TypedDict

import pytest
from itofin import ItofinError, Settings
from itofin.indexes import CpiInterpolationType, ZeroInflationIndex
from itofin.instruments import CpiCapFloor, OptionType
from itofin.pricingengines import InterpolatingCpiCapFloorEngine
from itofin.termstructures import CpiCapFloorTermPriceSurface, FlatForward, InterpolatedZeroInflationCurve
from itofin.time import BusinessDayConvention as Bdc
from itofin.time import Calendar, Date, DayCounter, Frequency, Period
from typing_extensions import Unpack

TODAY = Date(15, 6, 2025)


class _SurfaceArgs(TypedDict):
    """Statically check the heterogeneous surface constructor arguments."""

    nominal: float
    base_rate: float
    observation_lag: Period
    calendar: Calendar
    convention: Bdc
    day_counter: DayCounter
    index: ZeroInflationIndex
    interpolation: CpiInterpolationType
    discount: FlatForward
    cap_strikes: list[float]
    floor_strikes: list[float]
    maturity_years: list[int]
    cap_prices: list[list[float]]
    floor_prices: list[list[float]]
    settings: Settings


class _OptionArgs(TypedDict):
    """Statically check the cumulative option constructor arguments."""

    option_type: OptionType
    nominal: float
    start_date: Date
    base_cpi: float
    maturity: Date
    fixing_calendar: Calendar
    fixing_convention: Bdc
    payment_calendar: Calendar
    payment_convention: Bdc
    strike: float
    index: ZeroInflationIndex
    observation_lag: Period
    interpolation: CpiInterpolationType
    settings: Settings
    engine: InterpolatingCpiCapFloorEngine


class _SurfaceChanges(TypedDict, total=False):
    """Describe optional, precisely typed surface test overrides."""

    nominal: float
    base_rate: float
    observation_lag: Period
    calendar: Calendar
    convention: Bdc
    day_counter: DayCounter
    index: ZeroInflationIndex
    interpolation: CpiInterpolationType
    discount: FlatForward
    cap_strikes: list[float]
    floor_strikes: list[float]
    maturity_years: list[int]
    cap_prices: list[list[float]]
    floor_prices: list[list[float]]
    settings: Settings


class _OptionChanges(TypedDict, total=False):
    """Describe optional instrument test overrides without erasing types."""

    option_type: OptionType
    nominal: float
    start_date: Date
    base_cpi: float
    maturity: Date
    fixing_calendar: Calendar
    fixing_convention: Bdc
    payment_calendar: Calendar
    payment_convention: Bdc
    strike: float
    index: ZeroInflationIndex
    observation_lag: Period
    settings: Settings


def market():
    """Build the independent native fixture market."""
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    index = ZeroInflationIndex.uk_rpi(settings)
    for year, month, value in [
        (2024, 10, 99.5),
        (2024, 11, 99.8),
        (2024, 12, 100.0),
        (2025, 1, 100.2),
        (2025, 2, 100.4),
        (2025, 3, 100.6),
        (2025, 4, 100.8),
        (2025, 5, 101.0),
    ]:
        index.add_fixing(Date(1, month, year), value)
    dc = DayCounter.actual365_fixed()
    curve = InterpolatedZeroInflationCurve(
        TODAY, [Date(1, 1, 2025), Date(1, 1, 2035)], [0.025, 0.025], Frequency.Monthly, dc
    )
    index.link_to(curve)
    discount = FlatForward(TODAY, 0.04, dc)
    return settings, index, dc, discount


def surface(m, **changes: Unpack[_SurfaceChanges]):
    """Build a bounded immutable price surface."""
    settings, index, dc, discount = m
    args: _SurfaceArgs = {
        "nominal": 100000.0,
        "base_rate": 0.025,
        "observation_lag": Period(3, "Months"),
        "calendar": Calendar.null_calendar(),
        "convention": Bdc.Unadjusted,
        "day_counter": dc,
        "index": index,
        "interpolation": CpiInterpolationType.Flat,
        "discount": discount,
        "cap_strikes": [0.03, 0.04],
        "floor_strikes": [0.01, 0.02],
        "maturity_years": [1, 3, 5],
        "cap_prices": [[0.08, 0.15, 0.24], [0.04, 0.10, 0.16]],
        "floor_prices": [[0.02, 0.04, 0.06], [0.035, 0.065, 0.095]],
        "settings": settings,
    }
    args.update(**changes)
    return CpiCapFloorTermPriceSurface(**args)


def option(
    m, engine, kind=OptionType.Call, interpolation=CpiInterpolationType.Flat, lag=3, **changes: Unpack[_OptionChanges]
):
    """Build a cumulative CPI instrument retaining its engine."""
    settings, index, _, _ = m
    args: _OptionArgs = {
        "option_type": kind,
        "nominal": 100000.0,
        "start_date": TODAY,
        "base_cpi": 100.0,
        "maturity": Date(15, 6, 2027),
        "fixing_calendar": Calendar.null_calendar(),
        "fixing_convention": Bdc.Unadjusted,
        "payment_calendar": Calendar.null_calendar(),
        "payment_convention": Bdc.Unadjusted,
        "strike": 0.025,
        "index": index,
        "observation_lag": Period(lag, "Months"),
        "interpolation": interpolation,
        "settings": settings,
        "engine": engine,
    }
    args.update(**changes)
    return CpiCapFloor(**args)


def oracle():
    """Read the source-qualified native fixture."""
    path = Path(__file__).parent / "fixtures" / "cpi-capfloor-native.tsv"
    assert (
        hashlib.sha256(path.read_bytes()).hexdigest()
        == "e05bf46a6d323939ace0b267bb58c415ea8a4b0ac26839e673d393f0901f6b29"
    )
    canonical = Path(__file__).resolve().parents[3] / "sdk/go/testdata/cpi-capfloor-native.tsv"
    assert path.read_bytes() == canonical.read_bytes()
    provenance = json.loads(canonical.with_suffix(".provenance.json").read_text())
    assert provenance["fixture_sha256"] == hashlib.sha256(path.read_bytes()).hexdigest()
    with path.open() as stream:
        values = {row["name"]: float(row["value"]) for row in csv.DictReader(stream, delimiter="\t")}
    assert len(values) == provenance["row_count"] == 120
    assert all(math.isfinite(value) for value in values.values())
    return values


def test_native_surface_and_engine_fixture():
    """Match native grid prices and lag-adjusted engine values."""
    m = market()
    prices = surface(m)
    engine = InterpolatingCpiCapFloorEngine(prices)
    expected = oracle()
    count = 0
    for name, value in expected.items():
        parts = name.split("_")
        if parts[0] in ("cap", "floor") and len(parts) == 3:
            year, strike = int(parts[1]), float(parts[2])
            method = prices.cap_price if parts[0] == "cap" else prices.floor_price
            assert method(Date(15, 6, 2025 + year), strike) == pytest.approx(value, rel=0, abs=2e-14)
            count += 1
    assert count == 90
    for interpolation, label in [(CpiInterpolationType.Flat, "flat"), (CpiInterpolationType.Linear, "linear")]:
        for lag in [3, 5]:
            for kind, kind_label in [(OptionType.Call, "cap"), (OptionType.Put, "floor")]:
                actual = option(m, engine, kind, interpolation, lag).npv()
                assert actual == pytest.approx(expected[f"{label}_{kind_label}_lag{lag}"], rel=0, abs=2e-14)
        for changes in [dict(nominal=17.0), dict(base_cpi=321.0), dict(start_date=Date(15, 6, 2024))]:
            assert option(m, engine, interpolation=interpolation, **changes).npv() == pytest.approx(
                expected[f"{label}_cap_lag3"], rel=0, abs=2e-14
            )


def test_snapshot_curve_updates_roll_and_retained_lifetime():
    """Keep price snapshots immutable and dependencies alive."""
    m = market()
    prices = surface(m)
    before = prices.cap_price(Date(15, 6, 2027), 0.015)
    settings, index, dc, _ = m
    index.link_to(
        InterpolatedZeroInflationCurve(TODAY, [Date(1, 1, 2025), Date(1, 1, 2035)], [0.05, 0.05], Frequency.Monthly, dc)
    )
    changed = (settings, index, dc, FlatForward(TODAY, 0.08, dc))
    assert prices.cap_price(Date(15, 6, 2027), 0.015) == before
    assert surface(changed).cap_price(Date(15, 6, 2027), 0.015) == pytest.approx(
        oracle()["fresh_cap_after_update"], rel=0, abs=2e-14
    )
    engine = InterpolatingCpiCapFloorEngine(prices)
    instrument = option(m, engine)
    value = instrument.npv()
    del prices, engine, m, changed, index, dc
    gc.collect()
    assert instrument.npv() == value
    settings.set_evaluation_date(Date(16, 6, 2025))
    assert instrument.npv() == value
    settings.set_evaluation_date(Date(16, 6, 2027))
    assert instrument.is_expired()
    assert instrument.npv() == 0.0


@pytest.mark.parametrize(
    "changes",
    [
        dict(nominal=float("nan")),
        dict(cap_strikes=[0.03, float("inf")]),
        dict(cap_prices=[[0.0, 0.15, 0.24], [0.04, 0.10, 0.16]]),
        dict(cap_prices=[[0.08], [0.04]]),
        dict(maturity_years=[1, 3, 2]),
        dict(maturity_years=[1, 3, 2**31 - 1]),
        dict(observation_lag=Period(30, "Days")),
        dict(cap_strikes=[0.03, 1e308]),
    ],
)
def test_surface_invalid_inputs_are_normal_errors(changes):
    """Reject invalid input without poisoning later pricing."""
    m = market()
    with pytest.raises(ItofinError):
        surface(m, **changes)
    assert surface(m).cap_price(Date(15, 6, 2026), 0.03) == pytest.approx(0.08)


def test_instrument_invalid_lags_null_dates_and_calendar_horizon():
    """Reject unsupported lags, missing dates and mixed settings."""
    m = market()
    prices = surface(m)
    engine = InterpolatingCpiCapFloorEngine(prices)
    with pytest.raises(ItofinError):
        option(m, engine, lag=1).npv()
    with pytest.raises(ItofinError):
        option(m, engine, observation_lag=Period(30, "Days"))
    with pytest.raises(ItofinError):
        surface(m, calendar=Calendar.indonesia())
    with pytest.raises(ItofinError):
        option(m, engine, payment_calendar=Calendar.indonesia(), payment_convention=Bdc.Following)
    with pytest.raises(ItofinError):
        option(m, engine, fixing_calendar=Calendar.indonesia(), fixing_convention=Bdc.Following)
    other = Settings()
    other.set_evaluation_date(TODAY)
    with pytest.raises(ItofinError):
        option(m, engine, settings=other)
    assert option(m, engine).npv() == pytest.approx(oracle()["flat_cap_lag3"], rel=0, abs=2e-14)


def test_raw_maturity_expiry_can_precede_adjusted_payment():
    """Expire after raw maturity even when payment rolls to the next day."""
    m = market()
    instrument = option(
        m,
        InterpolatingCpiCapFloorEngine(surface(m)),
        maturity=Date(20, 6, 2027),
        payment_calendar=Calendar.weekends_only(),
        payment_convention=Bdc.Following,
    )
    assert instrument.pay_date() == Date(21, 6, 2027)
    m[0].set_evaluation_date(Date(20, 6, 2027))
    assert not instrument.is_expired()
    assert math.isfinite(instrument.npv())
    m[0].set_evaluation_date(Date(21, 6, 2027))
    assert instrument.is_expired()
    assert instrument.npv() == 0.0
