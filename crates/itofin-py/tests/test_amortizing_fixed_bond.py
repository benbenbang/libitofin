"""Amortizing fixed bonds: independently chosen native cases and facade boundaries."""

# standard library
import gc
import json
import math
from pathlib import Path
from typing import TypedDict, cast

# pypi/conda library
# third party
import pytest

# itofin library
from itofin import ItofinError, Settings
from itofin.indexes import Currency, IborIndex
from itofin.instruments import (
    AmortizingFixedRateBond,
    AssetSwap,
    Bond,
    BondForward,
    FixedRateBond,
    Position,
    ZeroCouponBond,
)
from itofin.pricingengines import DiscountingBondEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import BondPriceType, FlatForward
from itofin.time import BusinessDayConvention as BDC
from itofin.time import Calendar, Compounding, Date, DayCounter, Frequency, Period, Schedule

Snapshot = TypedDict(
    "Snapshot",
    {
        "values": list[float],
        "forward": list[float] | None,
        "settlement": int,
        "maturity": int,
        "expired": bool,
        "flows": list[list[float]],
        "redemptions": list[list[float]],
        "yield": float | None,
        "dated_notionals": list[float],
    },
)


class Row(TypedDict):
    name: str
    notionals: list[float]
    when: list[int]
    ex_days: int
    settlement_days: int
    initial: Snapshot
    updated: Snapshot


LINES = (Path(__file__).resolve().parents[3] / "sdk/go/testdata/amortizing_fixed_native.jsonl").read_text().splitlines()
PROVENANCE = json.loads(LINES[0])
ROWS = [cast(Row, json.loads(line)) for line in LINES[1:]]
NOTIONAL_DATES = [
    (16, 1, 2025),
    (17, 1, 2025),
    (18, 1, 2025),
    (18, 1, 2026),
    (19, 1, 2026),
    (20, 1, 2026),
    (17, 1, 2027),
    (18, 1, 2027),
    (19, 1, 2027),
]


def schedule() -> Schedule:
    """Keep unadjusted accrual dates distinct from adjusted principal payments."""
    return Schedule(Date(17, 1, 2024), Date(17, 1, 2027), Frequency.Annual, Calendar.weekends_only(), BDC.Unadjusted)


def make(
    settings: Settings, balances: list[float], ex_days: int = 0, settlement_days: int = 2
) -> AmortizingFixedRateBond:
    """Construct the bounded native-reference instrument."""
    return AmortizingFixedRateBond(
        settlement_days,
        balances,
        schedule(),
        [0.045, 0.052, 0.039],
        DayCounter.actual360(),
        settings,
        issue_date=Date(17, 1, 2024),
        ex_coupon_period=Period(ex_days, "Days") if ex_days else None,
        ex_coupon_calendar=Calendar.weekends_only(),
    )


def assert_snapshot(bond: AmortizingFixedRateBond, expected: Snapshot, curve: FlatForward) -> None:
    """Require finite values and compare currency, quotes, principal and yield."""
    values = [
        bond.npv(),
        bond.settlement_value(),
        bond.clean_price(),
        bond.dirty_price(),
        bond.accrued_amount(),
        bond.notional(),
    ]
    assert all(math.isfinite(value) for value in values)
    assert values == pytest.approx(expected["values"], abs=2e-10, rel=2e-12)
    assert (bond.settlement_date() - Date(1, 1, 1901) + 367) == expected["settlement"]
    assert (bond.maturity_date() - Date(1, 1, 1901) + 367) == expected["maturity"]
    assert bond.is_expired() == expected["expired"]
    for leg, rows in [(bond.cashflows(), expected["flows"]), (bond.redemptions(), expected["redemptions"])]:
        assert len(leg) == len(rows)
        for index, row in enumerate(rows):
            flow = leg[index]
            assert (flow.date() - Date(1, 1, 1901) + 367) == row[0]
            amount = flow.amount()
            assert math.isfinite(amount)
            assert amount == pytest.approx(row[1], abs=2e-11)
    for date, value in zip(NOTIONAL_DATES, expected["dated_notionals"], strict=True):
        assert bond.notional(Date(*date)) == value
    if expected["forward"] is not None:
        forward = BondForward(
            bond,
            Date(18, 1, 2024),
            Date(19, 1, 2026),
            Position.Long,
            105.0,
            0,
            DayCounter.actual360(),
            Calendar.weekends_only(),
            BDC.Unadjusted,
            curve,
        )
        actual = [forward.npv(), forward.fair_forward_price(), forward.clean_forward_price()]
        assert all(math.isfinite(value) for value in actual)
        assert actual == pytest.approx(expected["forward"], abs=2e-10, rel=2e-12)
    if expected["yield"] is not None:
        value = bond.yield_rate(
            values[2],
            DayCounter.actual360(),
            Compounding.Compounded,
            Frequency.Annual,
            accuracy=1e-12,
            max_evaluations=1000,
            guess=0.04,
        )
        assert math.isfinite(value)
        assert value == pytest.approx(expected["yield"], abs=3e-10)
        assert bond.clean_price_from_yield(
            value, DayCounter.actual360(), Compounding.Compounded, Frequency.Annual
        ) == pytest.approx(values[2], abs=3e-9)


@pytest.mark.parametrize("row", ROWS, ids=[row["name"] for row in ROWS])
def test_native_amortizing_fixed_snapshots(row: Row) -> None:
    """Read genuine whole-instrument QuantLib prices before and after quote changes."""
    assert PROVENANCE["compiled_quantlib_version"] == "1.43"
    assert PROVENANCE["source_is_compiled_wheel"] is False
    settings = Settings()
    settings.set_evaluation_date(Date(*row["when"]))
    bond = make(settings, row["notionals"], row["ex_days"], row["settlement_days"])
    assert isinstance(bond, Bond)
    assert not isinstance(bond, FixedRateBond)
    assert bond.frequency() == Frequency.Annual
    assert bond.day_counter() == DayCounter.actual360()
    assert bond.first_period_day_counter() is None
    quote = SimpleQuote(0.031)
    curve = FlatForward.from_quote(Date(18, 1, 2024), quote, DayCounter.actual360())
    bond.set_engine(DiscountingBondEngine(curve, settings))
    assert_snapshot(bond, row["initial"], curve)
    quote.set_value(0.067)
    assert_snapshot(bond, row["updated"], curve)


@pytest.mark.parametrize(
    "balances",
    [
        [],
        [1000.0],
        [1000.0, 750.0, 400.0, 1.0],
        [1000.0, 1001.0, 400.0],
        [1000.0, 0.0, 0.0],
        [1000.0, -1.0, -2.0],
        [math.inf, 750.0, 400.0],
        [1000.0, math.nan, 400.0],
        [1000.0, 750.0, -math.inf],
    ],
)
def test_amortizing_fixed_invalid_balances(balances: list[float]) -> None:
    """Reject inexact lengths, accretion and nonfinite or nonpositive balances."""
    with pytest.raises(ItofinError):
        make(Settings(), balances)


def test_amortizing_fixed_lifetime_settings_flags_and_consumer_boundary() -> None:
    """Retain the live graph and principal legs without enabling asset swaps."""
    settings = Settings()
    settings.set_evaluation_date(Date(18, 1, 2024))
    bond = make(settings, [1000.0, 750.0, 400.0])
    quote = SimpleQuote(0.031)
    curve = FlatForward.from_quote(Date(18, 1, 2024), quote, DayCounter.actual360())
    engine = DiscountingBondEngine(curve, settings)
    bond.set_engine(engine)
    index = IborIndex(
        "AmortizingBoundary",
        Period(1, "Years"),
        0,
        Currency.eur(),
        Calendar.weekends_only(),
        BDC.Unadjusted,
        False,
        DayCounter.actual360(),
        curve,
        settings,
    )
    with pytest.raises(ItofinError, match="redemption"):
        AssetSwap(True, bond, 100.0, index, 0.0)
    with pytest.raises(ItofinError, match="share settings"):
        bond.set_engine(DiscountingBondEngine(curve, Settings()))
    retained = bond.redemptions()
    initial = bond.npv()
    assert math.isfinite(initial) and initial > 0.0
    bond.freeze()
    quote.set_value(0.067)
    assert bond.npv() == initial
    bond.unfreeze()
    assert math.isfinite(bond.npv()) and bond.npv() < initial
    del engine, curve, index
    gc.collect()
    quote.set_value(0.02)
    updated = bond.npv()
    assert math.isfinite(updated) and updated > initial
    settings.set_evaluation_date(Date(20, 1, 2026))
    assert bond.notional() == 400.0
    settings.set_evaluation_date(Date(19, 1, 2027))
    assert bond.is_expired()
    assert bond.npv() == bond.clean_price() == bond.dirty_price() == 0.0
    del bond, quote, settings
    gc.collect()
    assert [retained[index].amount() for index in range(len(retained))] == [250.0, 350.0, 400.0]


def test_amortizing_fixed_options_and_domain_recovery() -> None:
    """Preserve first-period metadata, issue floors and exact calendar guards."""
    settings = Settings()
    settings.set_evaluation_date(Date(18, 1, 2024))
    bond = AmortizingFixedRateBond(
        2,
        [1000.0, 750.0, 400.0],
        schedule(),
        [0.045],
        DayCounter.actual360(),
        settings,
        issue_date=Date(1, 2, 2024),
        first_period_day_counter=DayCounter.actual365_fixed(),
    )
    assert bond.first_period_day_counter() == DayCounter.actual365_fixed()
    assert bond.settlement_date() == Date(1, 2, 2024)
    with pytest.raises(ItofinError, match="engine"):
        bond.npv()
    with pytest.raises(ItofinError):
        AmortizingFixedRateBond(0, [1000.0, 750.0, 400.0], schedule(), [math.nan], DayCounter.actual360(), settings)
    with pytest.raises(ItofinError):
        make(settings, [1000.0, 750.0, 400.0], -1)
    with pytest.raises(ItofinError):
        AmortizingFixedRateBond(2**32 - 1, [1000.0, 750.0, 400.0], schedule(), [0.04], DayCounter.actual360(), settings)
    with pytest.raises(ItofinError):
        AmortizingFixedRateBond(
            0,
            [1000.0, 750.0, 400.0],
            schedule(),
            [0.04],
            DayCounter.actual360(),
            settings,
            payment_calendar=Calendar.indonesia(),
        )
    bullet = FixedRateBond(0, 1000.0, schedule(), [0.04], DayCounter.actual360(), settings)
    assert len(bullet.redemptions()) == 1
    near = make(settings, [1000.0, 1000.0 - 1e-12, 400.0])
    assert len(near.redemptions()) == 2


def test_amortizing_fixed_silent_today_flags_and_ex_coupon_principal() -> None:
    """Detach coupons, not principal, and refresh silent flags without bypassing freeze."""
    settings = Settings()
    settings.set_evaluation_date(Date(17, 1, 2025))
    bond = make(settings, [1000.0, 750.0, 400.0], settlement_days=0)
    curve = FlatForward(Date(17, 1, 2025), 0.031, DayCounter.actual360())
    bond.set_engine(DiscountingBondEngine(curve, settings))
    before = bond.npv()
    bond.freeze()
    settings.set_include_reference_date_events(True)
    settings.set_include_todays_cash_flows(True)
    assert bond.npv() == before
    bond.unfreeze()
    assert bond.npv() - before == pytest.approx(250 + 1000 * 0.045 * 366 / 360, abs=2e-10)
    assert bond.notional() == 750.0
    settings.set_include_todays_cash_flows(False)
    assert bond.npv() == pytest.approx(before, abs=2e-10)
    settings.set_evaluation_date(Date(10, 1, 2025))
    settings.set_include_reference_date_events(False)
    settings.set_include_todays_cash_flows(None)
    plain = make(settings, [1000.0, 750.0, 400.0], settlement_days=0)
    detached = make(settings, [1000.0, 750.0, 400.0], ex_days=7, settlement_days=0)
    curve = FlatForward(Date(10, 1, 2025), 0.031, DayCounter.actual360())
    for instrument in [plain, detached]:
        instrument.set_engine(DiscountingBondEngine(curve, settings))
    assert plain.npv() - detached.npv() == pytest.approx(
        1000 * 0.045 * 366 / 360 * math.exp(-0.031 * 7 / 360), abs=2e-10
    )
    assert [detached.redemptions()[index].amount() for index in range(len(detached.redemptions()))] == [
        250.0,
        350.0,
        400.0,
    ]


@pytest.mark.parametrize("scale", [1e-250, 1e300])
def test_amortizing_fixed_extreme_balances_have_finite_current_notional_quotes(scale: float) -> None:
    """Retain the existing stable per100 scaling for very small and large faces."""
    settings = Settings()
    settings.set_evaluation_date(Date(18, 1, 2024))
    bond = AmortizingFixedRateBond(
        2, [scale, scale * 0.75, scale * 0.4], schedule(), [0.0], DayCounter.actual360(), settings
    )
    bond.set_engine(DiscountingBondEngine(FlatForward(Date(18, 1, 2024), 0.0, DayCounter.actual360()), settings))
    assert math.isfinite(bond.npv())
    assert bond.npv() == pytest.approx(scale, rel=2e-13, abs=0.0)
    assert bond.dirty_price() == pytest.approx(100.0, abs=2e-11)
    settings.set_evaluation_date(Date(20, 1, 2026))
    assert bond.notional() == pytest.approx(scale * 0.4, rel=2e-13, abs=0.0)
    assert math.isfinite(bond.dirty_price())
    assert bond.dirty_price() == pytest.approx(100.0, abs=2e-11)


@pytest.mark.parametrize("kind", ["fixed", "zero"])
def test_existing_bullet_zero_notional_yield_is_typed_nontradable_error(kind: str) -> None:
    """Preserve BondFunctions analytics errors, not native Bond class yield's zero fallback."""
    settings = Settings()
    settings.set_evaluation_date(Date(19, 1, 2027))
    counter = DayCounter.actual360()
    bond: Bond
    if kind == "fixed":
        bond = FixedRateBond(0, 1000.0, schedule(), [0.04], counter, settings)
    else:
        bond = ZeroCouponBond(0, Calendar.weekends_only(), 1000.0, Date(17, 1, 2027), settings)
    assert bond.notional() == 0.0
    for price_type in [BondPriceType.Clean, BondPriceType.Dirty]:
        with pytest.raises(ItofinError, match="non tradable"):
            bond.yield_rate(0.0, counter, Compounding.Compounded, Frequency.Annual, price_type=price_type)
