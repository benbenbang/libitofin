"""Live GJR-GARCH process inputs, state contracts and QuantLib transitions."""

import gc
import json
import math
from pathlib import Path

import numpy as np
import pytest

from itofin import ItofinError
from itofin.processes import GJRGARCHProcess
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward
from itofin.time import Date, DayCounter

REFERENCE = Date(15, 6, 2026)
SCHEMES = ("PartialTruncation", "FullTruncation", "Reflection")
BASE = dict(daily_variance=0.04 / 252, omega=2e-6, alpha=0.04, beta=0.88, gamma=0.08, lambda_=-0.4)
FIXTURES = Path(__file__).resolve().parents[3] / "sdk/go/testdata"


def market(scheme="FullTruncation", values=None, spot=100.0, rate=0.03, dividend=0.01):
    """Market."""
    quotes = [SimpleQuote(value) for value in (spot, rate, dividend)]
    dc = DayCounter.actual360()
    risk_free = FlatForward.from_quote(REFERENCE, quotes[1], dc)
    dividend_curve = FlatForward.from_quote(REFERENCE, quotes[2], dc)
    process = GJRGARCHProcess(quotes[0], risk_free, dividend_curve, **{**BASE, **(values or {})}, scheme=scheme)
    return process, quotes, risk_free, dividend_curve


@pytest.mark.parametrize("scheme", SCHEMES)
def test_parameters_state_shape_and_time(scheme):
    """Parameters state shape and time."""
    process, _, _, _ = market(scheme)
    for key, expected in BASE.items():
        assert getattr(process, key)() == expected
    assert process.days_per_year() == 252.0
    assert process.discretization() == scheme
    assert process.initial_values() == [100.0, 0.04]
    assert process.time(REFERENCE) == 0.0
    assert process.time(REFERENCE + 360) == 1.0
    state = process.initial_values()
    assert len(process.drift(0.0, state)) == 2
    assert np.asarray(process.diffusion(0.0, state)).shape == (2, 2)
    assert len(process.evolve(0.0, state, 0.01, [0.5, -0.4])) == 2


def test_live_spot_and_both_rate_quotes_and_lifetime():
    """Live spot and both rate quotes and lifetime."""
    process, quotes, risk_free, dividend = market()
    state = [100.0, 0.04]
    old = process.drift(0.0, state)
    quotes[0].set_value(123.0)
    assert process.initial_values() == [123.0, 0.04]
    quotes[1].set_value(0.05)
    assert process.drift(0.0, state)[0] == pytest.approx(old[0] + 0.02, abs=1e-12)
    quotes[2].set_value(0.02)
    assert process.drift(0.0, state)[0] == pytest.approx(old[0] + 0.01, abs=1e-12)
    rebuilt, _, _, _ = market(spot=123.0, rate=0.05, dividend=0.02)
    assert process.drift(0.1, state) == rebuilt.drift(0.1, state)
    expected = process.evolve(0.1, state, 0.01, [0.5, -0.4])
    del quotes, risk_free, dividend
    gc.collect()
    assert process.initial_values() == [123.0, 0.04]
    assert process.evolve(0.1, state, 0.01, [0.5, -0.4]) == expected


@pytest.mark.parametrize(
    "index,value", [(0, 0.0), (0, -1.0), (0, math.nan), (0, math.inf), (1, math.nan), (2, math.inf)]
)
def test_invalid_live_market_update_raises_and_recovers(index, value):
    """Invalid live market update raises and recovers."""
    process, quotes, _, _ = market()
    original = quotes[index].value()
    quotes[index].set_value(value)
    with pytest.raises(ItofinError):
        if index == 0:
            process.initial_values()
        else:
            process.evolve(0.0, [100.0, 0.04], 0.01, [0.0, 0.0])
    quotes[index].set_value(original)
    assert np.isfinite(process.initial_values()).all()
    assert np.isfinite(process.evolve(0.0, [100.0, 0.04], 0.01, [0.0, 0.0])).all()


@pytest.mark.parametrize(
    "values",
    [
        {"daily_variance": -0.01},
        {"omega": -1.0},
        {"alpha": -0.01},
        {"beta": -0.01},
        {"gamma": -0.1},
        {"days_per_year": 0.0},
        *[{key: value} for key in (*BASE, "days_per_year") for value in (math.nan, math.inf)],
    ],
)
def test_invalid_parameters(values):
    """Invalid parameters."""
    with pytest.raises(ItofinError):
        market(values=values)


@pytest.mark.parametrize("scheme", ["full", "fulltruncation", "", "QE"])
def test_unknown_scheme_rejected(scheme):
    """Unknown scheme rejected."""
    with pytest.raises(ItofinError):
        market(scheme)


@pytest.mark.parametrize(
    "state", [[], [100.0], [100.0, 0.04, 1.0], [0.0, 0.04], [-1.0, 0.04], [math.nan, 0.04], [100.0, math.nan]]
)
def test_state_validation_without_panics(state):
    """State validation without panics."""
    process, _, _, _ = market()
    for call in (
        lambda: process.drift(0.0, state),
        lambda: process.diffusion(0.0, state),
        lambda: process.evolve(0.0, state, 0.01, [0.0, 0.0]),
    ):
        with pytest.raises(ItofinError):
            call()


@pytest.mark.parametrize("draws", [[], [0.0], [0.0, 0.0, 0.0], [math.nan, 0.0], [0.0, math.inf]])
def test_normal_draw_validation(draws):
    """Normal draw validation."""
    process, _, _, _ = market()
    with pytest.raises(ItofinError):
        process.evolve(0.0, [100.0, 0.04], 0.01, draws)


@pytest.mark.parametrize(
    "t,dt", [(-1.0, 0.01), (math.nan, 0.01), (math.inf, 0.01), (0.0, -0.01), (0.0, math.nan), (0.0, math.inf)]
)
def test_time_validation(t, dt):
    """Time validation."""
    process, _, _, _ = market()
    with pytest.raises(ItofinError):
        process.evolve(t, [100.0, 0.04], dt, [0.0, 0.0])


TRANSITIONS = [
    case
    for name in ("0", "1", "2", "edge")
    for case in json.loads((FIXTURES / f"gjrgarch-transitions-{name}.json").read_text())["cases"]
]


@pytest.mark.parametrize("case", TRANSITIONS, ids=lambda case: case["name"])
def test_independent_quantlib_transitions(case):
    """Independent quantlib transitions."""
    inputs = dict(case["input"])
    scheme = SCHEMES[inputs.pop("discretization")]
    spot = inputs.pop("spot")
    rate = inputs.pop("risk_free_rate")
    dividend = inputs.pop("dividend_yield")
    inputs["lambda_"] = inputs.pop("lambda")
    process, _, _, _ = market(scheme, inputs, spot, rate, dividend)
    np.testing.assert_allclose(process.initial_values(), case["initial"], rtol=2e-13, atol=1e-12)
    np.testing.assert_allclose(process.drift(0.0, case["state"]), case["drift"], rtol=2e-13, atol=1e-12)
    np.testing.assert_allclose(process.diffusion(0.0, case["state"]), case["diffusion"], rtol=2e-13, atol=1e-12)
    np.testing.assert_allclose(
        process.evolve(0.0, case["state"], case["dt"], case["dw"]), case["evolve"], rtol=2e-13, atol=1e-12
    )


@pytest.mark.parametrize("value", [None, [[100.0, 0.04]], ["bad", 0.04]])
def test_malformed_state_and_draw_containers(value):
    """Reject nonnumeric or nested state/draw containers without native panic."""
    process, _, _, _ = market()
    with pytest.raises(TypeError):
        process.drift(0.0, value)
    with pytest.raises(TypeError):
        process.diffusion(0.0, value)
    with pytest.raises(TypeError):
        process.evolve(0.0, value, 0.01, [0.0, 0.0])
    with pytest.raises(TypeError):
        process.evolve(0.0, [100.0, 0.04], 0.01, value)
