"""Plain two-Ibor native-oracle values, typed ownership and lazy lifecycle."""

import csv
import gc
import json
import math
from pathlib import Path
from typing import TypedDict

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import Currency, IborIndex
from itofin.instruments import FloatFloatSwap
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention as BDC, Calendar, Date, DayCounter, Frequency, Period, Schedule

ROWS = list(csv.DictReader((Path(__file__).parent / "fixtures/float_float_swap_native.csv").read_text().splitlines()))


class OptionalSwapArgs(TypedDict, total=False):
    """Optional constructor terms kept distinct from required typed inputs."""

    payment_convention: BDC


class SwapArgs(OptionalSwapArgs):
    """Exact constructor shape for this controlled plain-product test fixture."""

    nominal: float
    pay_schedule: Schedule
    pay_index: IborIndex
    pay_spread: float
    pay_day_count: DayCounter
    receive_schedule: Schedule
    receive_index: IborIndex
    receive_spread: float
    receive_day_count: DayCounter
    settings: Settings


def market(
    row: dict[str, str] | None = None,
    attach: bool = True,
    *,
    nominal: float = 1_000_000.0,
    pay_spread: float | None = None,
    receive_spread: float | None = None,
) -> tuple[FloatFloatSwap, Settings, list[SimpleQuote], list[FlatForward], list[IborIndex], SwapArgs]:
    """Build the exact native default-engine profile with retained quote sources."""
    row = row or ROWS[0]
    settings = Settings()
    settings.set_evaluation_date(Date(2, 1, 2026))
    quotes = [SimpleQuote(float(row[key])) for key in ["forward1", "forward2", "discount"]]
    curves = [FlatForward.from_quote(Date(2, 1, 2026), q, DayCounter.actual360()) for q in quotes]
    indexes = [
        IborIndex(
            f"binding-float-{i}",
            Period(months, "Months"),
            0,
            Currency.eur(),
            Calendar.null_calendar(),
            BDC.Unadjusted,
            False,
            DayCounter.actual360(),
            curves[i],
            settings,
        )
        for i, months in enumerate([3, 6])
    ]
    schedules = [
        Schedule(Date(17, 1, 2026), Date(17, 1, 2027), frequency, Calendar.weekends_only(), BDC.Unadjusted)
        for frequency in [Frequency.Quarterly, Frequency.Semiannual]
    ]
    args: SwapArgs = {
        "nominal": nominal,
        "pay_schedule": schedules[0],
        "pay_index": indexes[0],
        "pay_spread": float(row["spread1"]) if pay_spread is None else pay_spread,
        "pay_day_count": DayCounter.actual360(),
        "receive_schedule": schedules[1],
        "receive_index": indexes[1],
        "receive_spread": float(row["spread2"]) if receive_spread is None else receive_spread,
        "receive_day_count": DayCounter.actual365_fixed(),
        "settings": settings,
    }
    if float(row["explicit"]):
        args["payment_convention"] = BDC.Following
    swap = FloatFloatSwap(**args)
    if attach:
        swap.set_engine(curves[2], settings)
    return swap, settings, quotes, curves, indexes, args


@pytest.mark.parametrize(
    "row", [r for r in ROWS if r["name"] in ["base", "negative", "following"]], ids=lambda r: r["name"]
)
def test_same_profile_native_oracle(row):
    """Direct engine matches only the legacy reference/settlement/NPV default profile."""
    swap, *_ = market(row)
    for name in ["npv", "pay_leg_npv", "receive_leg_npv", "pay_leg_bps", "receive_leg_bps"]:
        key = {"pay_leg_npv": "pv1", "receive_leg_npv": "pv2", "pay_leg_bps": "bps1", "receive_leg_bps": "bps2"}.get(
            name, name
        )
        assert getattr(swap, name)() == pytest.approx(float(row[key]), abs=2e-9, rel=0)
    assert swap.fair_pay_spread() == pytest.approx(float(row["fair1"]), abs=2e-13, rel=0)
    assert swap.fair_receive_spread() == pytest.approx(float(row["fair2"]), abs=2e-13, rel=0)
    assert swap.pay_leg_bps() < 0 < swap.receive_leg_bps()


def test_fair_absolute_spreads_and_bps_reprice():
    """Absolute fair spreads and signed one-basis-point moves reprice the same product."""
    swap, *_, args = market()
    paid, received = swap.fair_pay_spread(), swap.fair_receive_spread()
    assert abs(market(pay_spread=paid)[0].npv()) < 2e-9
    assert abs(market(receive_spread=received)[0].npv()) < 2e-9
    assert market(pay_spread=args["pay_spread"] + 1e-4)[0].npv() - swap.npv() == pytest.approx(
        swap.pay_leg_bps(), abs=2e-9, rel=0
    )
    assert market(receive_spread=args["receive_spread"] + 1e-4)[0].npv() - swap.npv() == pytest.approx(
        swap.receive_leg_bps(), abs=2e-9, rel=0
    )


def test_identity_missing_engine_and_replacement_failure():
    """Reject distinct Settings before replacing any installed native engine."""
    swap, settings, _, curves, _, args = market(attach=False)
    with pytest.raises(ItofinError):
        swap.npv()
    other = Settings()
    other.set_evaluation_date(Date(2, 1, 2026))
    other_args = args.copy()
    other_args["settings"] = other
    with pytest.raises(ItofinError, match="settings"):
        FloatFloatSwap(**other_args)
    with pytest.raises(ItofinError, match="settings"):
        swap.set_engine(curves[2], other)
    swap.set_engine(curves[2], settings)
    before = swap.npv()
    with pytest.raises(ItofinError, match="settings"):
        swap.set_engine(curves[0], other)
    assert swap.npv() == before


def test_live_quotes_fixings_freeze_recalculate_and_expiry():
    """Preserve native live inputs, explicit repricing and expired sensitivity errors."""
    swap, settings, quotes, _, indexes, _ = market()
    assert not swap.is_calculated()
    before = swap.npv()
    swap.freeze()
    quotes[0].set_value(0.05)
    assert swap.npv() == before
    swap.recalculate()
    assert abs(swap.npv() - before) > 1
    swap.unfreeze()
    settings.set_evaluation_date(Date(18, 1, 2026))
    with pytest.raises(ItofinError):
        swap.npv()
    for index in indexes:
        index.add_fixing(Date(17, 1, 2026), 0.03)
    fixed = swap.npv()
    with pytest.raises(ItofinError, match="duplicated"):
        indexes[0].add_fixing(Date(17, 1, 2026), 0.04)
    assert swap.npv() == fixed
    quotes[2].set_value(0.03)
    assert abs(swap.npv() - fixed) > 1
    settings.set_evaluation_date(Date(19, 1, 2027))
    assert swap.is_expired()
    assert [swap.npv(), swap.pay_leg_npv(), swap.receive_leg_npv(), swap.pay_leg_bps(), swap.receive_leg_bps()] == [
        0
    ] * 5
    with pytest.raises(ItofinError, match="sensitivity"):
        swap.fair_pay_spread()
    with pytest.raises(ItofinError, match="sensitivity"):
        swap.fair_receive_spread()


def test_dependencies_survive_facade_deletion():
    """Native coupon and engine graphs keep original curve inputs alive."""
    swap, settings, quotes, curves, indexes, args = market()
    before = swap.npv()
    del settings, curves, indexes, args
    gc.collect()
    quotes[1].set_value(0.04)
    assert abs(swap.npv() - before) > 1


@pytest.mark.parametrize("value", [0.0, -1.0, math.inf, math.nan])
def test_invalid_nominal(value):
    """Reject nonpositive or nonfinite common notionals."""
    with pytest.raises(ItofinError):
        market(nominal=value)


@pytest.mark.parametrize("field", ["pay_spread", "receive_spread"])
def test_invalid_spreads(field):
    """Reject nonfinite spreads on either leg."""
    with pytest.raises(ItofinError):
        if field == "pay_spread":
            market(pay_spread=math.nan)
        else:
            market(receive_spread=math.nan)


def test_never_calculated_freeze_and_silent_flags():
    """Preserve native unpriced freeze errors and silent-policy cache refresh."""
    swap, settings, _, _, _, _ = market()
    swap.freeze()
    with pytest.raises(ItofinError):
        swap.npv()
    swap.recalculate()
    before = swap.npv()
    settings.set_using_at_par_coupons(False)
    assert swap.npv() == before
    swap.unfreeze()
    assert math.isfinite(swap.npv())
    settings.set_using_at_par_coupons(True)
    assert swap.npv() == pytest.approx(before, abs=2e-9, rel=0)


def test_silent_today_and_reference_flow_policy_are_live():
    """Match an independent direct-engine oracle across six silent policy choices."""
    oracle = json.loads((Path(__file__).parent / "fixtures/float_float_swap_direct.json").read_text())
    assert oracle["version"] == "1.43"
    swap, settings, _, _, indexes, _ = market()
    settings.set_evaluation_date(Date(17, 7, 2026))
    for month in [1, 4]:
        indexes[0].add_fixing(Date(17, month, 2026), 0.021)
    indexes[1].add_fixing(Date(17, 1, 2026), 0.031)
    discount = FlatForward(Date(17, 7, 2026), 0.02, DayCounter.actual360())
    swap.set_engine(discount, settings)
    for row in oracle["rows"]:
        settings.set_include_reference_date_events(row["include_reference"])
        settings.set_include_todays_cash_flows(row["include_today"])
        for i, name in enumerate(oracle["measure_order"]):
            assert getattr(swap, name)() == pytest.approx(row["values"][i], abs=2e-9 if i < 5 else 2e-13, rel=0)


def test_omitted_payment_convention_preserves_each_schedule():
    """None keeps different paid and received schedule rules rather than one shared fallback."""
    swap, settings, _, curves, _, args = market()
    args["pay_schedule"] = Schedule(
        Date(17, 1, 2026), Date(17, 1, 2027), Frequency.Quarterly, Calendar.weekends_only(), BDC.Following
    )
    mixed = FloatFloatSwap(**args)
    mixed.set_engine(curves[2], settings)
    explicit_args = args.copy()
    explicit_args["payment_convention"] = BDC.Following
    explicit = FloatFloatSwap(**explicit_args)
    explicit.set_engine(curves[2], settings)
    assert mixed.pay_leg_npv() == pytest.approx(explicit.pay_leg_npv(), abs=2e-9, rel=0)
    assert abs(mixed.receive_leg_npv() - explicit.receive_leg_npv()) > 0.1
    assert math.isfinite(swap.npv())
