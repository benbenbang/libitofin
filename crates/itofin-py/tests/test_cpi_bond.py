"""Explicit-base CPI bonds against independently produced QuantLib values."""

import gc
import json
import math
from pathlib import Path
from typing import TypedDict, cast

import pytest

from itofin import ItofinError, Settings
from itofin.cashflows import npv, npv_at_z_spread, z_spread
from itofin.indexes import CpiInterpolationType, ZeroInflationIndex
from itofin.instruments import CPIBond, Bond, BondForward, Position
from itofin.pricingengines import DiscountingBondEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward, InterpolatedZeroInflationCurve
from itofin.time import BusinessDayConvention as BDC
from itofin.time import Calendar, Compounding, Date, DayCounter, Frequency, Period, Schedule


class Flow(TypedDict):
    """One native cash-flow observation."""

    date: str
    amount: float


class Snapshot(TypedDict):
    """Independent native bond values and dated cash flows."""

    npv: float
    clean_price: float
    dirty_price: float
    accrued_per100: float
    notional: float
    settlement: str
    maturity: str
    cashflows: list[Flow]


class Row(TypedDict):
    """Named native comparison profile."""

    name: str
    values: Snapshot


class Native(TypedDict):
    """Bounded bond fixture document."""

    bonds: list[Row]


FIXTURE = Path(__file__).resolve().parents[3] / "sdk/go/testdata/cpi-oracle-bonds.json"
ROWS = {row["name"]: row["values"] for row in cast(Native, json.loads(FIXTURE.read_text()))["bonds"] if "values" in row}
FIXINGS = [
    (10, 2024, 99.5),
    (11, 2024, 99.8),
    (12, 2024, 100.0),
    (1, 2025, 100.2),
    (2, 2025, 100.4),
    (3, 2025, 100.6),
    (4, 2025, 100.8),
    (5, 2025, 101.0),
]
TODAY = Date(15, 6, 2025)


def date(iso: str) -> Date:
    """Parse a fixture date without relying on serial-number conventions."""
    year, month, day = map(int, iso.split("-"))
    return Date(day, month, year)


def inflation(rate: float = 0.025) -> InterpolatedZeroInflationCurve:
    """Build the native input curve with independent explicit base date."""
    return InterpolatedZeroInflationCurve(
        TODAY, [Date(1, 1, 2025), Date(1, 1, 2035)], [rate, rate], Frequency.Monthly, DayCounter.actual365_fixed()
    )


def make(
    linear: bool = False, weekend: bool = False, apr: float = 100.8, omit_apr: bool = False
) -> tuple[CPIBond, ZeroInflationIndex, Settings, SimpleQuote]:
    """Build an instrument over the independent native fixture inputs."""
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    index = ZeroInflationIndex.uk_rpi(settings)
    index.link_to(inflation())
    for month, year, value in FIXINGS:
        if month == 4 and omit_apr:
            continue
        index.add_fixing(Date(1, month, year), apr if month == 4 else value)
    dc = DayCounter.actual365_fixed()
    quote = SimpleQuote(0.04)
    curve = FlatForward.from_quote(TODAY, quote, dc)
    bond = CPIBond(
        2,
        100000.0,
        100.0,
        Period(3, "Months"),
        index,
        CpiInterpolationType.Linear if linear else CpiInterpolationType.Flat,
        Schedule(
            Date(15, 1, 2025),
            Date(15, 7 if weekend else 1, 2028),
            Frequency.Semiannual,
            Calendar.null_calendar(),
            BDC.Unadjusted,
        ),
        [0.02],
        dc,
        settings,
        payment_convention=BDC.Following if weekend else BDC.Unadjusted,
        payment_calendar=Calendar.weekends_only() if weekend else Calendar.null_calendar(),
        issue_date=Date(15, 1, 2025),
        ex_coupon_period=Period(5, "Days") if weekend else None,
        ex_coupon_calendar=Calendar.weekends_only() if weekend else None,
    )
    bond.set_engine(DiscountingBondEngine(curve, settings))
    return bond, index, settings, quote


def assert_snapshot(bond: CPIBond, expected: Snapshot) -> None:
    """Compare pricing, date conventions and each retained cash-flow amount."""
    assert bond.npv() == pytest.approx(expected["npv"], rel=0, abs=2e-8)
    assert bond.clean_price() == pytest.approx(expected["clean_price"], rel=0, abs=2e-10)
    assert bond.dirty_price() == pytest.approx(expected["dirty_price"], rel=0, abs=2e-10)
    assert bond.accrued_amount() == pytest.approx(expected["accrued_per100"], rel=0, abs=2e-12)
    assert bond.notional() == expected["notional"]
    assert bond.settlement_date() == date(expected["settlement"])
    assert bond.maturity_date() == date(expected["maturity"])
    flows = bond.cashflows()
    assert len(flows) == len(expected["cashflows"])
    for position, row in enumerate(expected["cashflows"]):
        flow = flows[position]
        assert flow.date() == date(row["date"])
        assert flow.amount() == pytest.approx(row["amount"], rel=0, abs=2e-8)
    assert bond.redemptions()[0].amount() == pytest.approx(expected["cashflows"][-1]["amount"], rel=0, abs=2e-8)


@pytest.mark.parametrize("linear", [False, True])
def test_native_values_live_discount_and_relinked_inflation(linear: bool) -> None:
    """Compare native profiles and actual live curve updates."""
    bond, index, _, quote = make(linear)
    label = "linear" if linear else "flat"
    assert isinstance(bond, Bond)
    assert_snapshot(bond, ROWS[label])
    quote.set_value(0.05)
    assert_snapshot(bond, ROWS[label + "_discount_quote_5pct"])
    updated, updated_index, _, updated_quote = make(linear, apr=101.8)
    updated_quote.set_value(0.05)
    assert_snapshot(updated, ROWS[label + "_fixing_updated"])
    updated_index.link_to(inflation(0.03))
    assert_snapshot(updated, ROWS[label + "_inflation_relinked_3pct"])
    index.link_to(inflation(0.03))
    assert bond.npv() != pytest.approx(ROWS[label]["npv"], abs=1e-4)


def test_adjusted_terminal_date_and_negative_excoupon() -> None:
    """Keep contractual maturity separate from adjusted terminal payment."""
    bond, _, settings, _ = make(True, True)
    assert_snapshot(bond, ROWS["linear_adjusted_terminal_weekend"])
    assert bond.maturity_date() == Date(15, 7, 2028)
    assert bond.redemptions()[0].date() == Date(17, 7, 2028)
    settings.set_evaluation_date(Date(10, 7, 2025))
    assert_snapshot(bond, ROWS["linear_negative_excoupon"])
    assert bond.accrued_amount() < 0


def test_missing_fixings_are_lazy_and_live_and_dependencies_retained() -> None:
    """Exercise historical-fixing adoption and retained indexed principal."""
    bond, index, settings, quote = make(omit_apr=True)
    with pytest.raises(ItofinError, match="fixing"):
        bond.npv()
    index.add_fixing(Date(1, 4, 2025), 100.8)
    assert_snapshot(bond, ROWS["flat"])
    redemption = bond.redemptions()[0]
    retained = redemption.amount()
    del index, quote
    gc.collect()
    assert redemption.amount() == retained
    assert_snapshot(bond, ROWS["flat"])
    settings.set_evaluation_date(Date(16, 1, 2028))
    assert bond.npv() == 0.0
    with pytest.raises(ItofinError, match="fixing"):
        redemption.amount()


@pytest.mark.parametrize(
    "face,base,rates",
    [
        (0, 100, [0.02]),
        (math.inf, 100, [0.02]),
        (1, 0, [0.02]),
        (1, math.nan, [0.02]),
        (1, 100, []),
        (1, 100, [math.inf]),
    ],
)
def test_invalid_numeric_inputs(face: float, base: float, rates: list[float]) -> None:
    """Reject nonfinite or invalid construction inputs."""
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    index = ZeroInflationIndex.uk_rpi(settings)
    with pytest.raises(ItofinError):
        CPIBond(
            2,
            face,
            base,
            Period(3, "Months"),
            index,
            CpiInterpolationType.Flat,
            Schedule(
                Date(15, 1, 2025), Date(15, 1, 2028), Frequency.Semiannual, Calendar.null_calendar(), BDC.Unadjusted
            ),
            rates,
            DayCounter.actual365_fixed(),
            settings,
        )


def test_bounded_consumers_reject_before_resolving_missing_fixings() -> None:
    """Do not silently widen the existing fixed/zero-only forward consumer."""
    bond, _, _, _ = make(omit_apr=True)
    with pytest.raises(ItofinError, match="fixed or zero"):
        BondForward(
            bond,
            TODAY,
            Date(15, 8, 2025),
            Position.Long,
            100,
            0,
            DayCounter.actual365_fixed(),
            Calendar.null_calendar(),
            BDC.Unadjusted,
            FlatForward(TODAY, 0.04, DayCounter.actual365_fixed()),
        )


def test_freeze_preserves_cached_price_then_unfreeze_observes_quote() -> None:
    """Retain the ordinary Bond freeze and recalculation contract."""
    bond, _, _, quote = make()
    before = bond.npv()
    bond.freeze()
    quote.set_value(0.05)
    assert bond.npv() == before
    bond.unfreeze()
    assert_snapshot(bond, ROWS["flat_discount_quote_5pct"])


def test_retained_redemption_survives_bond_drop_and_stays_fixing_dependent() -> None:
    """Erasing the instrument does not erase the principal's live CPI graph."""
    bond, index, settings, quote = make()
    redemption = bond.redemptions()[0]
    expected = redemption.amount()
    del bond, index, quote
    gc.collect()
    assert redemption.amount() == expected
    settings.set_evaluation_date(Date(16, 1, 2028))
    with pytest.raises(ItofinError, match="fixing"):
        redemption.amount()


def test_settings_identity_is_checked_even_with_identical_dates() -> None:
    """Reject independently allocated Settings rather than comparing dates."""
    settings = Settings()
    settings.set_evaluation_date(TODAY)
    other = Settings()
    other.set_evaluation_date(TODAY)
    with pytest.raises(ItofinError, match="settings"):
        CPIBond(
            2,
            100000,
            100,
            Period(3, "Months"),
            ZeroInflationIndex.uk_rpi(other),
            CpiInterpolationType.Flat,
            Schedule(
                Date(15, 1, 2025), Date(15, 1, 2028), Frequency.Semiannual, Calendar.null_calendar(), BDC.Unadjusted
            ),
            [0.02],
            DayCounter.actual365_fixed(),
            settings,
        )


def test_extracted_legs_keep_settings_identity_after_bond_drop() -> None:
    """Both exported leg forms preserve CPI settings even after erasure."""
    bond, index, settings, quote = make()
    flows, redemptions = bond.cashflows(), bond.redemptions()
    del bond, index, quote
    gc.collect()
    curve = FlatForward(TODAY, 0.04, DayCounter.actual365_fixed())
    other = Settings()
    other.set_evaluation_date(TODAY)
    for leg in [flows, redemptions]:
        with pytest.raises(ValueError, match="settings"):
            npv(leg, curve, other)
        assert math.isfinite(npv(leg, curve, settings))
        with pytest.raises(ValueError, match="settings"):
            npv_at_z_spread(leg, curve, 0, Compounding.Continuous, Frequency.Annual, other)
        with pytest.raises(ValueError, match="settings"):
            z_spread(leg, 100000, curve, Compounding.Continuous, Frequency.Annual, other)


def test_extracted_cpi_leg_rejects_overflowing_finite_cashflow_sum() -> None:
    """A finite amount on each flow does not make an overflowing sum valid."""
    _, index, settings, _ = make()
    huge = CPIBond(
        2,
        8e307,
        100,
        Period(3, "Months"),
        index,
        CpiInterpolationType.Flat,
        Schedule(Date(15, 1, 2025), Date(15, 1, 2028), Frequency.Semiannual, Calendar.null_calendar(), BDC.Unadjusted),
        [1.0],
        DayCounter.actual365_fixed(),
        settings,
        payment_convention=BDC.Unadjusted,
    )
    flows = huge.cashflows()
    assert all(math.isfinite(flows[i].amount()) for i in range(len(flows)))
    with pytest.raises(ItofinError, match="finite"):
        npv(flows, FlatForward(TODAY, 0.04, DayCounter.actual365_fixed()), settings)


class Terminal(TypedDict):
    """Native preceding-payment notional and event-inclusion observations."""

    include_reference_date_events: bool
    notional_jul17: float
    notional_jul18: float
    notional_jul19: float
    expired: bool
    cashflow_npv_include_settlement: float
    cashflow_npv_exclude_settlement: float


class PrecedingRow(TypedDict):
    """A native schedule, coupon vector and terminal observations."""

    tenor_months: int
    rates: list[float]
    initial: Snapshot
    terminal: list[Terminal]


class PrecedingFixture(TypedDict):
    """Independently produced preceding-payment comparison profiles."""

    rows: list[PrecedingRow]


def test_native_preceding_payment_and_terminal_notional_shutdown() -> None:
    """Keep the indexed observation at maturity even if payment precedes it."""
    fixture = FIXTURE.with_name("cpi-bond-preceding.json")
    rows = cast(PrecedingFixture, json.loads(fixture.read_text()))["rows"]
    for row in rows:
        _, index, settings, _ = make()
        dc = DayCounter.actual365_fixed()
        curve = FlatForward(TODAY, 0.04, dc)
        bond = CPIBond(
            0,
            1000,
            100,
            Period(3, "Months"),
            index,
            CpiInterpolationType.Linear,
            Schedule(
                Date(15, 1, 2025),
                Date(19, 7, 2025),
                Frequency.Annual if row["tenor_months"] == 12 else Frequency.Semiannual,
                Calendar.null_calendar(),
                BDC.Unadjusted,
            ),
            row["rates"],
            dc,
            settings,
            payment_convention=BDC.Preceding,
            payment_calendar=Calendar.weekends_only(),
            issue_date=Date(15, 1, 2025),
        )
        bond.set_engine(DiscountingBondEngine(curve, settings))
        assert_snapshot(bond, row["initial"])
        flows = bond.cashflows()
        for terminal in row["terminal"]:
            settings.set_include_reference_date_events(terminal["include_reference_date_events"])
            settings.set_evaluation_date(Date(18, 7, 2025))
            assert bond.notional(Date(17, 7, 2025)) == terminal["notional_jul17"]
            assert bond.notional(Date(18, 7, 2025)) == terminal["notional_jul18"]
            assert bond.notional(Date(19, 7, 2025)) == terminal["notional_jul19"]
            assert bond.is_expired() == terminal["expired"]
            assert npv(flows, curve, settings, True, Date(18, 7, 2025), Date(18, 7, 2025)) == pytest.approx(
                terminal["cashflow_npv_include_settlement"], rel=0, abs=2e-10
            )
            assert npv(flows, curve, settings, False, Date(18, 7, 2025), Date(18, 7, 2025)) == pytest.approx(
                terminal["cashflow_npv_exclude_settlement"], rel=0, abs=2e-10
            )


def test_raw_cpi_coupon_bps_and_atm_are_explicitly_unsupported() -> None:
    """Do not mislabel generic coupon weights as raw CPI coupon sensitivity."""
    bond, _, _, _ = make(omit_apr=True)
    curve = FlatForward(TODAY, 0.04, DayCounter.actual365_fixed())
    with pytest.raises(ItofinError, match="CPI.*unsupported"):
        bond.bps_from_curve(curve)
    with pytest.raises(ItofinError, match="CPI.*unsupported"):
        bond.bps_from_yield(0.04, DayCounter.actual365_fixed(), Compounding.Continuous, Frequency.Annual)
    with pytest.raises(ItofinError, match="CPI.*unsupported"):
        bond.atm_rate_from_curve(curve)
    with pytest.raises(ItofinError, match="CPI.*unsupported"):
        bond.atm_rate_from_price(curve, 100)
