"""QuantLib test-suite/bermudanswaption.cpp six 50-step Hull-White cached pins."""

import gc
import json
from pathlib import Path

import pytest

from itofin import ItofinError, Settings
from itofin.indexes import Estr, Euribor
from itofin.instruments import (
    BermudanExercise, EuropeanExercise, MakeOis, SettlementMethod,
    SettlementType, Swaption, SwapType, VanillaSwap,
)
from itofin.models import HullWhite
from itofin.pricingengines import TreeSwaptionEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward
from itofin.time import BusinessDayConvention as BDC
from itofin.time import Calendar, Date, DayCounter, Frequency, Period, Schedule


ORACLE = json.loads((Path(__file__).resolve().parents[3] / "sdk/go/testdata/tree_swaption_oracle.json").read_text())
assert ORACLE["quantlib"] == "1.43"
assert len(ORACLE["rows"]) == 6

CACHED = [(False, [42.2402, 12.9032, 2.49758]), (True, [42.2460, 12.9069, 2.4985])]


def _market(at_par):
    settings = Settings()
    settings.set_evaluation_date(Date(15, 2, 2002))
    assert settings.using_at_par_coupons()
    settings.set_using_at_par_coupons(at_par)
    assert settings.using_at_par_coupons() == at_par
    cal = Calendar.target()
    settlement = cal.advance(Date(15, 2, 2002), 2, "Days", BDC.Following, False)
    quote = SimpleQuote(0.04875825)
    curve = FlatForward.from_quote(settlement, quote, DayCounter.actual365_fixed())
    index = Euribor.six_months(curve, settings)
    start = cal.advance(settlement, 1, "Years", BDC.Following, False)
    end = cal.advance(start, 5, "Years", BDC.Following, False)
    fixed = Schedule(start, end, Frequency.Annual, cal, BDC.Unadjusted)
    floating = Schedule(start, end, Frequency.Semiannual, cal, BDC.ModifiedFollowing)

    def swap(rate):
        result = VanillaSwap(
            SwapType.Payer, 1000.0, fixed, rate, DayCounter.thirty360_bond_basis(),
            floating, index, 0.0, DayCounter.actual360(), settings,
        )
        result.set_engine(curve, settings)
        return result

    return settings, quote, curve, fixed, swap


def _option(at_par, multiplier):
    settings, quote, curve, fixed, make_swap = _market(at_par)
    rate = make_swap(0.0).fair_rate()
    swap = make_swap(multiplier * rate)
    exercise = BermudanExercise(fixed.dates()[:-1])
    model = HullWhite(curve, 0.048696, 0.0058904)
    engine = TreeSwaptionEngine(model, 50, settings)
    option = Swaption.from_bermudan(
        swap, exercise, SettlementType.Physical, SettlementMethod.PhysicalOTC, settings,
    )
    option.set_tree_engine(engine)
    return option, swap, quote


@pytest.mark.parametrize("at_par, values", CACHED)
@pytest.mark.parametrize("index, multiplier", enumerate([0.8, 1.0, 1.2]))
def test_tree_swaption_six_quantlib_cached_values(at_par, values, index, multiplier):
    option, swap, quote = _option(at_par, multiplier)
    source_npv = swap.npv()
    source_rate = swap.fixed_rate()
    gc.collect()
    assert option.npv() == pytest.approx(values[index], rel=0, abs=1e-4)
    assert swap.npv() == source_npv
    assert swap.fixed_rate() == source_rate
    quote.set_value(0.06)
    row = next(row for row in ORACLE["rows"] if row["at_par"] == at_par and row["multiplier"] == multiplier)
    assert option.npv() == pytest.approx(row["bumped"], rel=0, abs=1e-4)
    quote.set_value(0.04875825)
    del swap, quote
    gc.collect()
    assert option.npv() == pytest.approx(values[index], rel=0, abs=1e-4)


def test_tree_swaption_cold_price_retains_all_inputs():
    option, swap, quote = _option(False, 1.0)
    del swap, quote
    gc.collect()
    assert option.npv() == pytest.approx(12.9032, rel=0, abs=1e-4)


def test_bermudan_dates_are_sorted_copied_and_empty_is_recoverable():
    first, last = Date(15, 2, 2003), Date(15, 2, 2004)
    dates = [last, first, first]
    with pytest.raises(ItofinError, match="no exercise date"):
        BermudanExercise([])
    exercise = BermudanExercise(dates)
    dates.clear()
    copied = exercise.dates()
    assert copied == [first, first, last]
    copied.clear()
    assert exercise.dates() == [first, first, last]


def test_tree_swaption_invalid_steps_cash_and_ois_do_not_poison_pricing():
    settings, _, curve, fixed, make_swap = _market(True)
    model = HullWhite(curve, 0.048696, 0.0058904)
    with pytest.raises(ItofinError, match="positive"):
        TreeSwaptionEngine(model, 0, settings)
    with pytest.raises(OverflowError):
        TreeSwaptionEngine(model, -1, settings)
    engine = TreeSwaptionEngine(model, 50, settings)
    exercise = BermudanExercise(fixed.dates()[:-1])
    swap = make_swap(0.05)
    cash = Swaption.from_bermudan(
        swap, exercise, SettlementType.Cash, SettlementMethod.ParYieldCurve, settings,
    )
    cash.set_tree_engine(engine)
    with pytest.raises(ItofinError, match="ParYieldCurve"):
        cash.npv()
    ois = MakeOis(
        Period(5, "Years"), Estr(curve, settings), settings,
        fixed_rate=0.05, effective_date=fixed.dates()[0],
    ).build()
    option = Swaption.from_ois(
        ois, EuropeanExercise(fixed.dates()[0]),
        SettlementType.Physical, SettlementMethod.PhysicalOTC, settings,
    )
    option.set_tree_engine(engine)
    with pytest.raises(ItofinError, match="vanilla Ibor swap"):
        option.npv()
    valid = Swaption.from_bermudan(
        swap, exercise, SettlementType.Physical, SettlementMethod.PhysicalOTC, settings,
    )
    valid.set_tree_engine(engine)
    assert valid.npv() > 0
