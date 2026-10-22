"""Black delta conventions, core QuantLib fixture and immutable boundaries."""
import json
import math
from pathlib import Path
from typing import Any, cast

import pytest

from itofin import ItofinError
from itofin.instruments import OptionType
from itofin.pricingengines import BlackDeltaCalculator
from itofin.quotes import AtmType, DeltaType

DELTAS = [DeltaType.Spot, DeltaType.Fwd, DeltaType.PaSpot, DeltaType.PaFwd]
ATMS = [AtmType.Null, AtmType.Spot, AtmType.Fwd, AtmType.DeltaNeutral,
        AtmType.VegaMax, AtmType.GammaMax, AtmType.PutCall50]


def calc(option=OptionType.Call, convention=DeltaType.Spot, deviation=.1180654):
    return BlackDeltaCalculator(option, convention, 1.421, .997306, .992266, deviation)


def test_export_and_stable_enum_values():
    assert [int(x) for x in DELTAS] == list(range(4))
    assert [int(x) for x in ATMS] == list(range(7))
    assert int(OptionType.Call) == 0
    assert int(OptionType.Put) == 1
    assert BlackDeltaCalculator.__module__ == "itofin.pricingengines"
    assert DeltaType.__module__ == AtmType.__module__ == "itofin.quotes"


def test_core_compiled_quantlib_fixture():
    path = Path(__file__).resolve().parents[3] / "sdk/go/testdata/black_delta.json"
    rows = json.loads(path.read_text())["rows"]
    assert len(rows) == 8
    for row in rows:
        option = [OptionType.Call, OptionType.Put][row["option"]]
        c = calc(option, DELTAS[row["convention"]])
        assert c.delta_from_strike(1.6) == pytest.approx(row["delta"], abs=1e-14, rel=0)
        signed = .15 if row["option"] == 0 else -.15
        strike = c.strike_from_delta(signed)
        assert strike == pytest.approx(row["strike"], abs=1e-10, rel=0)
        assert c.delta_from_strike(strike) == pytest.approx(signed, abs=1e-8 * abs(signed), rel=0)


@pytest.mark.parametrize("convention", DELTAS)
@pytest.mark.parametrize("option", [OptionType.Call, OptionType.Put])
def test_all_atm_conventions(option, convention):
    c = calc(option, convention)
    for atm in ATMS:
        if atm == AtmType.Null or (atm == AtmType.PutCall50 and convention != DeltaType.Fwd):
            with pytest.raises(ItofinError):
                c.atm_strike(atm)
            continue
        want = 1.4237071536606858
        if atm == AtmType.Spot:
            want = 1.421
        elif atm == AtmType.Fwd:
            want = 1.413818813884605
        elif atm == AtmType.DeltaNeutral and convention in DELTAS[2:]:
            want = 1.4039991534456169
        assert c.atm_strike(atm) == pytest.approx(want, abs=1e-14, rel=0)


@pytest.mark.parametrize("convention", DELTAS)
def test_put_call_parity_and_premium_branch(convention):
    call, put = calc(convention=convention), calc(OptionType.Put, convention)
    scale = .992266 if convention in [DeltaType.Spot, DeltaType.PaSpot] else 1
    want = scale if convention in DELTAS[:2] else scale * 1.6 / 1.413818813884605
    assert call.delta_from_strike(1.6) - put.delta_from_strike(1.6) == pytest.approx(want, abs=1e-14, rel=0)
    if convention in DELTAS[2:]:
        low = call.delta_from_strike(.5)
        high = call.strike_from_delta(low)
        assert high > 1.4
        assert call.delta_from_strike(high) == pytest.approx(low, abs=1e-10, rel=0)
        with pytest.raises(ItofinError):
            call.strike_from_delta(.9)


@pytest.mark.parametrize("convention", DELTAS)
@pytest.mark.parametrize("deviation", [0, 1e-20])
def test_zero_small_vol_and_unclamped_put(convention, deviation):
    c = BlackDeltaCalculator(OptionType.Put, convention, 1, 1, 1, deviation)
    assert c.delta_from_strike(1) == -.5
    assert c.delta_from_strike(2) == (-1 if convention in DELTAS[:2] else -2)
    with pytest.raises(ItofinError):
        c.strike_from_delta(-.25)


@pytest.mark.parametrize("parameter", range(4))
@pytest.mark.parametrize("bad", [math.nan, math.inf, -math.inf, -1])
def test_invalid_constructor(parameter, bad):
    args = [1, 1, 1, .1]
    args[parameter] = bad
    with pytest.raises(ItofinError):
        BlackDeltaCalculator(OptionType.Call, DeltaType.Fwd, *args)


def test_domains_types_and_immutable_state():
    c = calc(convention=DeltaType.Fwd)
    for bad in [math.nan, math.inf, -1]:
        with pytest.raises(ItofinError):
            c.delta_from_strike(bad)
    for bad in [math.nan, math.inf, -.1, 0, 1, 2]:
        with pytest.raises(ItofinError):
            c.strike_from_delta(bad)
    with pytest.raises(TypeError):
        BlackDeltaCalculator(cast(Any, 0), DeltaType.Fwd, 1, 1, 1, .1)
    with pytest.raises(TypeError):
        BlackDeltaCalculator(OptionType.Call, cast(Any, 1), 1, 1, 1, .1)
    with pytest.raises(TypeError):
        c.atm_strike(cast(Any, 2))
    assert not hasattr(c, "set_delta_type")
    assert not hasattr(c, "set_option_type")
    with pytest.raises(AttributeError):
        setattr(c, "std_dev", 1)
    assert c.atm_strike(AtmType.Fwd) == pytest.approx(1.413818813884605, abs=1e-14, rel=0)


def test_extreme_finite_and_zero_discount_domains():
    for args in [(0, 1, 1, .1), (1, 0, 1, .1), (1, 1, 0, .1),
                 (1e308, 1e-300, 1, .1), (1, 1, 1, 1e308)]:
        with pytest.raises(ItofinError):
            BlackDeltaCalculator(OptionType.Call, DeltaType.Fwd, *args)
    tiny = BlackDeltaCalculator(OptionType.Call, DeltaType.Fwd, 1e-300, 1, 1, .1)
    with pytest.raises(ItofinError):
        tiny.delta_from_strike(1e308)
