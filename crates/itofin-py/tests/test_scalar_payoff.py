"""Standalone payoff terms, native trigger rules and finite boundary contract."""

import csv
import math
from pathlib import Path
import sys
from concurrent.futures import ThreadPoolExecutor
from typing import Any, cast

import pytest

from itofin import ItofinError
from itofin.instruments import OptionType, PayoffKind, ScalarPayoff

FACTORIES = [
    (ScalarPayoff.plain_vanilla, PayoffKind.PlainVanilla, 100, None),
    (ScalarPayoff.cash_or_nothing, PayoffKind.CashOrNothing, 100, -7),
    (ScalarPayoff.asset_or_nothing, PayoffKind.AssetOrNothing, 100, None),
    (ScalarPayoff.gap, PayoffKind.Gap, 100, 110),
    (ScalarPayoff.percentage_strike, PayoffKind.PercentageStrike, 0.8, None),
]


def create(factory, option, strike, extra):
    return factory(option, strike) if extra is None else factory(option, strike, extra)


@pytest.mark.parametrize("option", [OptionType.Call, OptionType.Put])
@pytest.mark.parametrize("factory,kind,strike,extra", FACTORIES)
def test_scalar_payoff_copied_terms_and_equality_trigger(factory, kind, strike, extra, option):
    payoff = create(factory, option, strike, extra)
    assert payoff.kind() == kind
    assert payoff.option_type() == option
    assert payoff.strike() == strike
    assert payoff.cash_payoff() == (extra if kind == PayoffKind.CashOrNothing else None)
    assert payoff.second_strike() == (extra if kind == PayoffKind.Gap else None)
    if kind in [PayoffKind.PlainVanilla, PayoffKind.CashOrNothing, PayoffKind.AssetOrNothing]:
        assert payoff.value(strike) == 0
    if kind == PayoffKind.Gap:
        assert payoff.value(strike) == (-10 if option == OptionType.Call else 10)
    with pytest.raises(AttributeError):
        setattr(payoff, "extra", 1)


@pytest.mark.parametrize("option", [OptionType.Call, OptionType.Put])
def test_scalar_payoff_strict_binary_and_signed_values(option):
    cash = ScalarPayoff.cash_or_nothing(option, 100, -7)
    asset = ScalarPayoff.asset_or_nothing(option, 100)
    inside = math.nextafter(100, math.inf if option == OptionType.Call else -math.inf)
    outside = math.nextafter(100, -math.inf if option == OptionType.Call else math.inf)
    assert cash.value(inside) == -7
    assert asset.value(inside) == inside
    assert cash.value(outside) == 0
    assert asset.value(outside) == 0
    assert ScalarPayoff.asset_or_nothing(OptionType.Call, -2).value(-1) == -1
    assert ScalarPayoff.plain_vanilla(OptionType.Put, -1).value(-2) == 1
    assert ScalarPayoff.percentage_strike(OptionType.Call, 0.8).value(-100) == pytest.approx(-20)
    assert ScalarPayoff.percentage_strike(OptionType.Put, 1.2).value(-100) == pytest.approx(-20)


@pytest.mark.parametrize("factory,kind,strike,extra", FACTORIES)
@pytest.mark.parametrize("bad", [math.nan, math.inf, -math.inf])
def test_scalar_payoff_rejects_nonfinite_terms_and_prices(factory, kind, strike, extra, bad):
    with pytest.raises(ItofinError, match="parameters must be finite"):
        create(factory, OptionType.Call, bad, extra)
    if extra is not None:
        with pytest.raises(ItofinError, match="parameters must be finite"):
            create(factory, OptionType.Call, strike, bad)
    payoff = create(factory, OptionType.Call, strike, extra)
    with pytest.raises(ItofinError, match="price must be finite"):
        payoff.value(bad)


def test_scalar_payoff_overflow_and_nonconstructible_classes():
    vanilla = ScalarPayoff.plain_vanilla(OptionType.Call, -sys.float_info.max)
    with pytest.raises(ItofinError, match="value must be finite"):
        vanilla.value(sys.float_info.max)
    ratio = ScalarPayoff.percentage_strike(OptionType.Put, sys.float_info.max)
    with pytest.raises(ItofinError, match="value must be finite"):
        ratio.value(2)
    with pytest.raises(TypeError):
        cast(Any, ScalarPayoff)()
    with pytest.raises(TypeError):
        cast(Any, PayoffKind)(0)
    with pytest.raises(TypeError):
        cast(Any, ScalarPayoff.plain_vanilla)(0, 100)
    with pytest.raises(TypeError):
        cast(Any, ScalarPayoff.plain_vanilla(OptionType.Call, 100).value)("100")


def test_scalar_payoff_immutable_threaded_reads_and_kind_numbers():
    kinds = [PayoffKind.PlainVanilla, PayoffKind.CashOrNothing, PayoffKind.AssetOrNothing, PayoffKind.Gap, PayoffKind.PercentageStrike]
    assert [int(kind) for kind in kinds] == list(range(5))
    payoff = ScalarPayoff.gap(OptionType.Call, 100, 110)
    with ThreadPoolExecutor(max_workers=4) as pool:
        assert list(pool.map(payoff.value, [100] * 32)) == [-10] * 32


def test_scalar_payoff_native_fixtures():
    path = Path(__file__).resolve().parents[3] / "sdk/go/testdata/scalar_payoffs.tsv"
    with path.open() as stream:
        rows = list(csv.DictReader(stream, delimiter="\t"))
    assert len(rows) == 384
    for row in rows:
        factory, _, _, default_extra = FACTORIES[int(row["kind"])]
        option = OptionType.Call if int(row["option"]) == 0 else OptionType.Put
        extra = None if default_extra is None else float(row["extra"])
        payoff = create(factory, option, float(row["strike"]), extra)
        assert payoff.value(float(row["price"])) == float(row["value"])
