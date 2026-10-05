"""Pinned independent native variance-swap values and purchased option weights."""

import json
import math
from pathlib import Path

import pytest

from itofin import Settings
from itofin.instruments import OptionType, Position, VarianceSwap
from itofin.pricingengines import ReplicatingVarianceSwapEngine
from itofin.processes import BlackScholesProcess
from itofin.termstructures import BlackConstantVol, BlackVarianceSurface, FlatForward
from itofin.time import Date, DayCounter

FIXTURES = Path(__file__).resolve().parents[3] / "sdk/go/testdata/variance-swap"
MANIFEST = json.loads((FIXTURES / "oracle.json").read_text())
CASES = [json.loads((FIXTURES / filename).read_text()) for filename in MANIFEST["cases"]]


def date(value):
    """Translate the independent fixture's explicit ISO calendar date."""
    year, month, day = map(int, value.split("-"))
    return Date(day, month, year)


def instrument(inputs):
    """Construct the exact native market without changing variance units."""
    settings = Settings()
    reference = date(inputs["evaluation_date"])
    settings.set_evaluation_date(reference)
    dc = DayCounter.actual365_fixed()
    assert inputs["day_counter"] == "Actual365Fixed"
    maturity = reference + inputs["maturity_days"]
    assert maturity == date(inputs["maturity_date"])
    risk_free = FlatForward(reference, inputs["risk_free_rate"], dc)
    dividend = FlatForward(reference, inputs["dividend_yield"], dc)
    if "smile_strikes" in inputs:
        vol = BlackVarianceSurface(
            reference,
            [maturity],
            inputs["smile_strikes"],
            [[value] for value in inputs["smile_vols"]],
            dc,
        )
    else:
        vol = BlackConstantVol(reference, inputs["volatility"], dc)
    process = BlackScholesProcess.from_curves(inputs["spot"], risk_free, dividend, vol)
    engine = ReplicatingVarianceSwapEngine(process, inputs["call_strikes"], inputs["put_strikes"], dk=inputs["dk"])
    position = Position.Long if inputs["position"] == "long" else Position.Short
    assert inputs["position"] in ("long", "short")
    swap = VarianceSwap(
        position,
        inputs["variance_strike"],
        inputs["notional"],
        date(inputs["start_date"]),
        maturity,
        settings,
    )
    swap.set_engine(engine)
    return swap


def assert_native(swap, native):
    """Check finite scalars and every typed purchased strike and weight."""
    variance, npv = swap.variance(), swap.npv()
    assert math.isfinite(variance) and math.isfinite(npv)
    assert variance == pytest.approx(native["variance"], rel=3e-12, abs=2e-14)
    assert npv == pytest.approx(native["npv"], rel=3e-12, abs=2e-9)
    weights = swap.option_weights()
    assert len(weights) == len(native["weights"])
    for actual, expected in zip(weights, native["weights"]):
        assert expected["option_type"] in ("call", "put")
        kind = OptionType.Call if expected["option_type"] == "call" else OptionType.Put
        assert actual[0] == kind
        assert actual[1] == expected["strike"]
        assert math.isfinite(actual[2])
        assert actual[2] == pytest.approx(expected["weight"], rel=3e-12, abs=2e-14)


@pytest.mark.parametrize("case", CASES, ids=lambda case: case["name"])
def test_independent_pinned_native_fixture_prices_and_full_option_strip(case):
    """All generated cases use upstream outputs rather than binding self-oracles."""
    assert_native(instrument(case["inputs"]), case["native"])


@pytest.mark.parametrize("case", [case for case in CASES if case.get("updates")], ids=lambda case: case["name"])
def test_native_explicitly_refreshed_update_markets_match_reconstructed_binding(case):
    """Native cached staleness is not copied as the binding's intended behavior."""
    for update in case["updates"]:
        inputs = {**case["inputs"], **update["market"]}
        expected = {**case["native"], **update["recalculated"]}
        assert_native(instrument(inputs), expected)
