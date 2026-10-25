"""Native CPI-versus-Ibor fixtures, par spread and retained live dependencies."""

import json
import gc
import math
from pathlib import Path
from typing import Any, TypedDict, cast

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import CpiInterpolationType, Currency, IborIndex, ZeroInflationIndex
from itofin.instruments import CPISwap, SwapType
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward, InterpolatedZeroInflationCurve
from itofin.time import BusinessDayConvention as BDC, Calendar, Date, DayCounter, Frequency, Period, Schedule

ORACLE = Path(__file__).parents[3] / "sdk/go/testdata"
PROFILES = json.loads((ORACLE / "cpi-oracle-swaps.json").read_text())["swaps"]
ROWS = [dict(profile, side=side, expected=values) for profile in PROFILES for side, values in profile["values"].items()]
assert len(PROFILES) == 8 and len(ROWS) == 16
assert all(set(profile["values"]) == {"payer", "receiver"} for profile in PROFILES)


class SwapArgs(TypedDict):
    """Exact public CPISwap constructor inputs."""

    swap_type: SwapType
    nominal: float
    subtract_inflation_nominal: bool
    spread: float
    float_schedule: Schedule
    float_index: IborIndex
    float_day_count: DayCounter
    float_convention: BDC
    fixed_rate: float
    base_cpi: float
    fixed_schedule: Schedule
    fixed_index: ZeroInflationIndex
    fixed_day_count: DayCounter
    fixed_convention: BDC
    observation_lag: Period
    settings: Settings
    interpolation: CpiInterpolationType
    inflation_nominal: float


def market(
    row: dict[str, Any] | None = None, spread: float = 0.001, attach: bool = True
) -> tuple[CPISwap, Settings, SimpleQuote, FlatForward, IborIndex, ZeroInflationIndex, SwapArgs]:
    """Match the independent wheel fixture's curves, dates and GBP index conventions."""
    row = row or ROWS[0]
    settings = Settings()
    today = Date(15, 6, 2025)
    settings.set_evaluation_date(today)
    dc = DayCounter.actual365_fixed()
    curve = InterpolatedZeroInflationCurve(
        today, [Date(1, 1, 2025), Date(1, 1, 2035)], [0.025, 0.025], Frequency.Monthly, dc
    )
    index = ZeroInflationIndex.uk_rpi(settings)
    index.link_to(curve)
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
    quote = SimpleQuote(0.04)
    discount = FlatForward.from_quote(today, quote, dc)
    floating = IborIndex(
        "CpiSwapGBP",
        Period(6, "Months"),
        0,
        Currency.gbp(),
        Calendar.united_kingdom("Exchange"),
        BDC.ModifiedFollowing,
        True,
        dc,
        discount,
        settings,
    )
    floating.add_fixing(Date(15, 1, 2025), 0.042)
    schedule = Schedule(
        Date(15, 1, 2025), Date(15, 1, 2028), Frequency.Semiannual, Calendar.null_calendar(), BDC.Unadjusted
    )
    args: SwapArgs = {
        "swap_type": SwapType.Payer if row["side"] == "payer" else SwapType.Receiver,
        "nominal": 100000.0,
        "subtract_inflation_nominal": row["growth_only"],
        "spread": spread,
        "float_schedule": schedule,
        "float_index": floating,
        "float_day_count": dc,
        "float_convention": BDC.Unadjusted,
        "fixed_rate": 0.02,
        "base_cpi": 100.0,
        "fixed_schedule": schedule,
        "fixed_index": index,
        "fixed_day_count": dc,
        "fixed_convention": BDC.Unadjusted,
        "observation_lag": Period(3, "Months"),
        "settings": settings,
        "interpolation": CpiInterpolationType.Flat if row["interpolation"] == "flat" else CpiInterpolationType.Linear,
        "inflation_nominal": float(row["inflation_nominal"]),
    }
    swap = CPISwap(**args)
    if attach:
        swap.set_engine(discount, settings)
    return swap, settings, quote, discount, floating, index, args


@pytest.mark.parametrize("row", ROWS)
def test_independent_native_profiles(row):
    """Match both signed sides and reprice the absolute fair Ibor spread."""
    swap, *_ = market(row)
    for method, key in [
        ("npv", "npv"),
        ("fixed_leg_npv", "cpi_leg_npv"),
        ("float_leg_npv", "floating_leg_npv"),
        ("float_leg_bps", "floating_leg_bps"),
        ("fair_spread", "fair_spread"),
    ]:
        assert getattr(swap, method)() == pytest.approx(
            float(row["expected"][key]), abs=2e-13 if method == "fair_spread" else 2e-9, rel=0
        )
    assert abs(market(row, spread=swap.fair_spread())[0].npv()) < 2e-9
    assert market(row, spread=0.0011)[0].npv() - swap.npv() == pytest.approx(swap.float_leg_bps(), abs=2e-9, rel=0)


def test_live_lifecycle_and_retained_dependencies():
    """Reprice live sources and retain them after facade wrappers are dropped."""
    swap, settings, quote, discount, floating, index, args = market()
    assert not swap.is_calculated()
    initial = swap.npv()
    swap.freeze()
    quote.set_value(0.05)
    assert swap.npv() == initial
    swap.recalculate()
    assert abs(swap.npv() - initial) > 1
    swap.unfreeze()
    index.link_to(
        InterpolatedZeroInflationCurve(
            Date(15, 6, 2025),
            [Date(1, 1, 2025), Date(1, 1, 2035)],
            [0.03, 0.03],
            Frequency.Monthly,
            DayCounter.actual365_fixed(),
        )
    )
    changed = swap.npv()
    assert abs(changed - initial) > 1
    del discount, floating, index, args
    gc.collect()
    quote.set_value(0.04)
    assert math.isfinite(swap.npv())
    settings.set_evaluation_date(Date(16, 1, 2028))
    assert swap.is_expired()
    assert swap.npv() == 0
    with pytest.raises(ItofinError):
        swap.fair_spread()


def test_numeric_settings_and_missing_engine_guards():
    """Reject bad inputs and wrong Settings without changing a valid engine."""
    swap, settings, _, discount, _, _, args = market(attach=False)
    with pytest.raises(ItofinError):
        swap.npv()
    other = Settings()
    other.set_evaluation_date(Date(15, 6, 2025))
    with pytest.raises(ItofinError, match="settings"):
        CPISwap(**cast(SwapArgs, args | {"settings": other}))
    for key in ["nominal", "inflation_nominal", "base_cpi"]:
        for value in [0.0, -1.0, float("nan"), float("inf")]:
            with pytest.raises(ItofinError):
                CPISwap(**cast(SwapArgs, args | {key: value}))
    for key in ["spread", "fixed_rate"]:
        with pytest.raises(ItofinError):
            CPISwap(**cast(SwapArgs, args | {key: float("nan")}))
    swap.set_engine(discount, settings)
    initial = swap.npv()
    with pytest.raises(ItofinError, match="settings"):
        swap.set_engine(discount, other)
    assert swap.npv() == initial


def test_zero_coupon_rate_does_not_remove_indexed_principal_dependency():
    """Even a zero-rate CPI leg still needs its final indexed principal fixing."""
    _, settings, _, discount, _, _, args = market()
    empty_index = ZeroInflationIndex.uk_rpi(settings)
    swap = CPISwap(**cast(SwapArgs, args | {"fixed_rate": 0.0, "fixed_index": empty_index}))
    swap.set_engine(discount, settings)
    with pytest.raises(ItofinError):
        swap.npv()
