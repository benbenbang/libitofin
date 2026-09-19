"""European Black swaption prices and ownership against independent QuantLib pins.

The cached value is QuantLib's test-suite/swaption.cpp testCachedValue.
Settlement rows reuse sdk/go/testdata/rates_completion_oracle.json, generated
independently by its sibling script with QuantLib 1.43. Absolute NPV tolerance
is 1e-12 for both, matching the Rust cached-value and Go settlement gates.
"""

import gc
import json
from pathlib import Path

import pytest

from itofin import Settings
from itofin.indexes import Euribor
from itofin.instruments import (
    EuropeanExercise,
    MakeVanillaSwap,
    SettlementMethod,
    SettlementType,
    Swaption,
    SwapType,
    VanillaSwap,
)
from itofin.pricingengines import BlackSwaptionEngine, CashAnnuityModel
from itofin.quotes import SimpleQuote
from itofin.termstructures import ConstantSwaptionVolatility, FlatForward, VolatilityType
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    DateGeneration,
    DayCounter,
    Frequency,
    Period,
    Schedule,
)

ORACLE_PATH = Path(__file__).resolve().parents[3] / "sdk/go/testdata/rates_completion_oracle.json"
ORACLE = json.loads(ORACLE_PATH.read_text())
assert ORACLE["quantlib"] == "1.43"
assert len(ORACLE["swaptions"]) == 12


def _cached_option(surface_engine):
    settings = Settings()
    today = Date(13, 3, 2002)
    settings.set_evaluation_date(today)
    calendar = Calendar.target()
    dc = DayCounter.actual365_fixed()
    following = BusinessDayConvention.Following
    settlement = calendar.advance(today, 2, "Days", following, False)
    curve = FlatForward(settlement, 0.05, dc)
    index = Euribor.six_months(curve, settings)
    exercise_date = calendar.advance(settlement, 5, "Years", following, False)
    start = calendar.advance(exercise_date, 2, "Days", following, False)
    swap = MakeVanillaSwap(
        Period(10, "Years"), index, settings,
        fixed_rate=0.06,
        effective_date=start,
        fixed_leg_tenor=Period(1, "Years"),
        fixed_leg_day_count=DayCounter.thirty360_bond_basis(),
    ).build()
    option = Swaption(
        swap, EuropeanExercise(exercise_date),
        SettlementType.Physical, SettlementMethod.PhysicalOTC, settings,
    )
    quote = SimpleQuote(0.20)
    if surface_engine:
        surface = ConstantSwaptionVolatility.moving_with_quote(
            0, Calendar.null_calendar(), following, quote, dc,
            VolatilityType.ShiftedLognormal, settings,
        )
        engine = BlackSwaptionEngine(surface, curve, settings)
    else:
        engine = BlackSwaptionEngine.with_flat_vol(curve, quote, dc, 0.0, settings)
    option.set_black_engine(engine)
    return option, quote


@pytest.mark.parametrize("surface_engine", [False, True], ids=["flat", "surface"])
def test_black_cached_value_retains_dependencies_and_observes_quote(surface_engine):
    option, quote = _cached_option(surface_engine)
    gc.collect()
    initial = option.npv()
    assert initial == pytest.approx(0.036418158579, rel=0, abs=1e-12)
    quote.set_value(0.30)
    assert option.npv() > initial
    quote.set_value(0.20)
    del quote
    gc.collect()
    assert option.npv() == pytest.approx(0.036418158579, rel=0, abs=1e-12)


def _settlement_option(row):
    today = Date(7, 7, 2026)
    settings = Settings()
    settings.set_evaluation_date(today)
    calendar = Calendar.target()
    dc = DayCounter.actual365_fixed()
    curve = FlatForward(today, 0.04, dc)
    index = Euribor.six_months(curve, settings)
    following = BusinessDayConvention.Following
    exercise = calendar.advance(today, 1, "Years", following, False)
    start = calendar.advance(exercise, 2, "Days", following, False)
    end = calendar.advance(start, 3, "Years", following, False)
    schedules = [
        Schedule(
            start, end, frequency, calendar,
            BusinessDayConvention.ModifiedFollowing, DateGeneration.Backward,
        )
        for frequency in [Frequency.Annual, Frequency.Semiannual]
    ]
    swap_type = {"payer": SwapType.Payer, "receiver": SwapType.Receiver}[row["side"]]
    swap = VanillaSwap(
        swap_type, 1.0, schedules[0], 0.04, dc, schedules[1], index,
        0.0, DayCounter.actual360(), settings,
    )
    settlements = {
        "physical_cleared": (SettlementType.Physical, SettlementMethod.PhysicalCleared),
        "collateralized_cash": (SettlementType.Cash, SettlementMethod.CollateralizedCashPrice),
        "par_yield": (SettlementType.Cash, SettlementMethod.ParYieldCurve),
    }
    settlement_type, method = settlements[row["settlement"]]
    option = Swaption(swap, EuropeanExercise(exercise), settlement_type, method, settings)
    model = {
        "swap_rate": CashAnnuityModel.SwapRate,
        "discount_curve": CashAnnuityModel.DiscountCurve,
    }[row["annuity"]]
    option.set_black_engine(
        BlackSwaptionEngine.with_flat_vol(curve, SimpleQuote(0.20), dc, 0.0, settings, model)
    )
    return option


@pytest.mark.parametrize(
    "row", ORACLE["swaptions"],
    ids=lambda row: f"{row['side']}/{row['settlement']}/{row['annuity']}",
)
def test_black_settlement_prices_match_quantlib_143(row):
    option = _settlement_option(row)
    gc.collect()
    assert option.npv() == pytest.approx(row["npv"], rel=0, abs=1e-12)
