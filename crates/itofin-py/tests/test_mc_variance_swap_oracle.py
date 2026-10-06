"""Pinned source-built native MC outputs with unchanged statistical bands."""

import json
import math
from pathlib import Path

import pytest

from itofin import Settings
from itofin.instruments import Position, VarianceSwap
from itofin.pricingengines import MCVarianceSwapEngine
from itofin.processes import BlackScholesProcess
from itofin.quotes import SimpleQuote
from itofin.termstructures import BlackConstantVol, BlackVarianceCurve, FlatForward
from itofin.time import Date, DayCounter

FIXTURES = Path(__file__).resolve().parents[3] / "sdk/go/testdata/mc-variance-swap"
CASES = json.loads((FIXTURES / "cases.json").read_text())["cases"]
LIVE = json.loads((FIXTURES / "live_updates.json").read_text())


def date(value):
    """Translate explicit native ISO dates without using global settings."""
    year, month, day = map(int, value.split("-"))
    return Date(day, month, year)


def instrument(inputs):
    """Reconstruct the published native market and exact engine selectors."""
    reference = date(inputs["evaluation_date"])
    settings = Settings()
    settings.set_evaluation_date(reference)
    dc = DayCounter.actual365_fixed()
    if inputs["volatility_kind"] == "variance_curve":
        vol = BlackVarianceCurve(
            reference,
            [reference + days for days in inputs["curve_days"]],
            inputs["curve_vols"],
            dc,
            True,
        )
    else:
        assert inputs["volatility_kind"] == "constant"
        vol = BlackConstantVol(reference, inputs["volatility"], dc)
    process = BlackScholesProcess.from_curves(
        inputs["spot"],
        FlatForward(reference, inputs["risk_free_rate"], dc),
        FlatForward(reference, inputs["dividend_yield"], dc),
        vol,
    )
    engine = MCVarianceSwapEngine(
        process,
        seed=inputs["seed"],
        **{key: inputs.get(key) or None for key in ("steps", "steps_per_year", "samples", "tolerance", "max_samples")},
    )
    assert inputs["position"] in ("long", "short")
    swap = VarianceSwap(
        Position.Long if inputs["position"] == "long" else Position.Short,
        inputs["variance_strike"],
        inputs["notional"],
        date(inputs["start_date"]),
        reference + inputs["maturity_days"],
        settings,
    )
    swap.set_engine(engine)
    return swap


def assert_native(swap, native):
    """Cash and variance tolerances are independent of the literature band."""
    for getter, field, absolute in (
        (swap.variance, "variance", 2e-14),
        (swap.npv, "npv", 2e-9),
        (swap.variance_error, "variance_error", 2e-14),
        (swap.error_estimate, "error_estimate", 2e-9),
    ):
        actual = getter()
        assert math.isfinite(actual)
        assert actual == pytest.approx(native[field], rel=3e-12, abs=absolute)
    assert swap.samples() == native["samples"]
    assert swap.option_weights() == []


@pytest.mark.parametrize("case", CASES, ids=lambda case: case["name"])
def test_source_built_native_mc_fixture_results_and_actual_sampling_count(case):
    """All cases compare against independent native executable output."""
    swap = instrument(case["inputs"])
    assert_native(swap, case["native"])
    swap.recalculate()
    assert_native(swap, case["native"])


def test_native_retained_live_quotes_and_reconstructed_scalar_spot_updates():
    """Quote edits invalidate retained engines; scalar spot uses reconstruction."""
    inputs = LIVE["inputs"]
    reference = date(inputs["evaluation_date"])
    settings = Settings()
    settings.set_evaluation_date(reference)
    dc = DayCounter.actual365_fixed()
    fields = ("risk_free_rate", "dividend_yield", "volatility")
    quotes = [SimpleQuote(inputs[field]) for field in fields]
    process = BlackScholesProcess.from_curves(
        inputs["spot"],
        FlatForward.from_quote(reference, quotes[0], dc),
        FlatForward.from_quote(reference, quotes[1], dc),
        BlackConstantVol.from_quote(reference, quotes[2], dc),
    )
    engine = MCVarianceSwapEngine(
        process, steps_per_year=inputs["steps_per_year"], samples=inputs["samples"], seed=inputs["seed"]
    )
    swap = VarianceSwap(
        Position.Long,
        inputs["variance_strike"],
        inputs["notional"],
        reference,
        reference + inputs["maturity_days"],
        settings,
    )
    swap.set_engine(engine)
    assert_native(swap, LIVE["native"])
    for update in LIVE["updates"]:
        market = update["market"]
        if market["spot"] != inputs["spot"]:
            assert_native(instrument({**inputs, **market}), update["native"])
            continue
        for field, quote in zip(fields, quotes):
            quote.set_value(market[field])
        assert not swap.is_calculated()
        assert_native(swap, update["native"])
        for field, quote in zip(fields, quotes):
            quote.set_value(inputs[field])
        assert_native(swap, LIVE["native"])
