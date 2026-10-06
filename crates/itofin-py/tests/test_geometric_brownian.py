"""Named scalar GBM process preserves additive Euler and signed state semantics."""

import gc
import json
import math
from pathlib import Path
from typing import TypedDict

import numpy as np
import pytest

import itofin
from itofin import ItofinError
from itofin.processes import GeometricBrownianMotionProcess


class ExactGbmConfiguration(TypedDict):
    """Typed inputs for the unchanged exact-lognormal simulation regression."""

    initial: list[float]
    drift: list[float]
    volatility: list[float]
    correlation: list[float]
    horizon: float
    steps: int
    paths: int
    seed: int


@pytest.mark.parametrize("initial", [100.0, 0.0, -100.0])
def test_constructor_copies_parameters_and_has_no_date_conversion(initial):
    """The independent scalar process needs no session, quote, curve or date."""
    process = GeometricBrownianMotionProcess(initial=initial, mu=0.05, volatility=0.2)
    assert process.x0() == initial
    assert process.mu() == 0.05
    assert process.volatility() == 0.2
    assert not hasattr(process, "time")
    assert not hasattr(process, "set_value")
    assert GeometricBrownianMotionProcess.__module__ == "itofin.processes"
    gc.collect()
    assert process.x0() == initial


@pytest.mark.parametrize("x", [100.0, 0.0, -100.0])
def test_coefficients_and_euler_moments_accept_signed_states(x):
    """State-dependent diffusion and deviation are signed, unlike variance."""
    process = GeometricBrownianMotionProcess(17.0, 0.05, 0.2)
    assert process.drift(0.3, x) == 0.05 * x
    assert process.diffusion(0.3, x) == 0.2 * x
    assert process.expectation(0.3, x, 0.25) == x + 0.05 * x * 0.25
    assert process.std_deviation(0.3, x, 0.25) == 0.2 * x * 0.5
    assert process.variance(0.3, x, 0.25) == (0.2 * x) ** 2 * 0.25
    expected = x + 0.05 * x * 0.25 + 0.2 * x * 0.5 * -1.5
    assert process.evolve(0.3, x, 0.25, -1.5) == expected
    assert process.evolve(9.0, x, 0.25, -1.5) == expected


def test_signed_euler_draw_can_cross_zero_without_clamping():
    """Additive composition preserves raw negative states and signed deviation."""
    process = GeometricBrownianMotionProcess(100.0, 0.0, 1.0)
    assert process.evolve(0.0, 100.0, 1.0, -2.0) == -100.0
    assert process.evolve(0.0, -100.0, 1.0, -2.0) == 100.0
    assert process.std_deviation(0.0, -100.0, 1.0) == -100.0
    assert process.variance(0.0, -100.0, 1.0) == 10000.0
    assert process.evolve(0.0, 0.0, 1.0, -1e300) == 0.0


def test_zero_time_and_zero_volatility_keep_euler_drift():
    """Zero-time transitions retain their input; deterministic growth is Euler."""
    process = GeometricBrownianMotionProcess(100.0, 0.2, 0.0)
    assert process.expectation(0.0, 100.0, 0.0) == 100.0
    assert process.variance(0.0, 100.0, 0.0) == 0.0
    assert process.std_deviation(0.0, 100.0, 0.0) == 0.0
    assert process.evolve(0.0, 100.0, 0.0, 1e300) == 100.0
    assert process.evolve(0.0, 100.0, 1.0, 1e300) == 120.0
    assert process.evolve(0.0, -100.0, 1.0, 1e300) == -120.0


@pytest.mark.parametrize(
    "field,value",
    [(field, value) for field in ("initial", "mu", "volatility") for value in (math.nan, math.inf, -math.inf)]
    + [("volatility", -0.1)],
)
def test_invalid_constructor_inputs_raise_itofin_error(field, value):
    """The constructor rejects nonfinite coefficients and negative volatility."""
    parameters = dict(initial=100.0, mu=0.05, volatility=0.2)
    parameters[field] = value
    with pytest.raises(ItofinError):
        GeometricBrownianMotionProcess(**parameters)


@pytest.mark.parametrize("method", ["drift", "diffusion"])
@pytest.mark.parametrize("t,x", [(-0.1, 100.0), (math.nan, 100.0), (math.inf, 100.0), (0.0, math.nan), (0.0, math.inf)])
def test_invalid_coefficient_queries_raise_and_recover(method, t, x):
    """Invalid query arguments do not mutate the copied process coefficients."""
    process = GeometricBrownianMotionProcess(100.0, 0.05, 0.2)
    operation = getattr(process, method)
    with pytest.raises(ItofinError):
        operation(t, x)
    assert math.isfinite(operation(0.0, 100.0))
    assert process.x0() == 100.0


@pytest.mark.parametrize("method", ["expectation", "variance", "std_deviation", "evolve"])
@pytest.mark.parametrize(
    "t,x,dt",
    [
        (-0.1, 100.0, 0.25),
        (math.nan, 100.0, 0.25),
        (math.inf, 100.0, 0.25),
        (0.0, math.nan, 0.25),
        (0.0, math.inf, 0.25),
        (0.0, 100.0, -0.25),
        (0.0, 100.0, math.nan),
        (0.0, 100.0, math.inf),
    ],
)
def test_invalid_transition_queries_raise_and_recover(method, t, x, dt):
    """Finite nonnegative time intervals are required by every Euler method."""
    process = GeometricBrownianMotionProcess(100.0, 0.05, 0.2)
    operation = getattr(process, method)
    extra = [0.0] if method == "evolve" else []
    with pytest.raises(ItofinError):
        operation(t, x, dt, *extra)
    assert math.isfinite(operation(0.0, 100.0, 0.25, *extra))
    assert process.x0() == 100.0


@pytest.mark.parametrize("dw", [math.nan, math.inf, -math.inf])
def test_nonfinite_draw_rejected_even_for_zero_time(dw):
    """Invalid draws are errors even when the diffusion contribution is zero."""
    process = GeometricBrownianMotionProcess(100.0, 0.0, 0.0)
    with pytest.raises(ItofinError):
        process.evolve(0.0, 100.0, 0.0, dw)
    assert process.evolve(0.0, 100.0, 0.0, 0.0) == 100.0


@pytest.mark.parametrize("method", ["drift", "diffusion", "expectation", "variance", "std_deviation", "evolve"])
def test_finite_inputs_with_overflow_raise(method):
    """Overflow cannot escape as a nonfinite public process result."""
    process = GeometricBrownianMotionProcess(1.0, 1e200, 1e200)
    arguments = [0.0, 1e200]
    if method not in ("drift", "diffusion"):
        arguments.append(1.0)
    if method == "evolve":
        arguments.append(1.0)
    with pytest.raises(ItofinError):
        getattr(process, method)(*arguments)
    assert process.drift(0.0, 1.0) == 1e200


def test_process_collection_does_not_invalidate_another_process():
    """Independent immutable instances do not share mutable ownership or state."""
    first = GeometricBrownianMotionProcess(100.0, 0.05, 0.2)
    second = GeometricBrownianMotionProcess(75.0, -0.03, 0.4)
    expected = second.evolve(0.0, 75.0, 0.5, 0.7)
    del first
    gc.collect()
    assert second.x0() == 75.0
    assert second.evolve(0.0, 75.0, 0.5, 0.7) == expected


def test_existing_exact_simulate_gbm_retains_seeded_bits_and_terminal_layout():
    """The named Euler process neither replaces nor changes the exact path API."""
    configuration = ExactGbmConfiguration(
        initial=[100.0, 70.0],
        drift=[0.05, -0.02],
        volatility=[0.2, 0.3],
        correlation=[1.0, 0.5, 0.5, 1.0],
        horizon=1.0,
        steps=12,
        paths=4,
        seed=42,
    )
    before = itofin.simulate_gbm(**configuration)
    process = GeometricBrownianMotionProcess(100.0, 0.05, 0.2)
    assert process.expectation(0.0, 100.0, 1.0) == 105.0
    assert process.expectation(0.0, 100.0, 1.0) != pytest.approx(100.0 * math.exp(0.05))
    after = itofin.simulate_gbm(**configuration)
    terminal = itofin.simulate_gbm(**configuration, terminal_only=True)
    assert before.shape == (4, 13, 2)
    assert after.dtype == np.float64
    assert np.array_equal(before.view(np.uint64), after.view(np.uint64))
    assert np.array_equal(terminal.view(np.uint64), after[:, -1, :].view(np.uint64))
    assert before.ravel()[:4].view(np.uint64).tolist() == [
        0x4059000000000000,
        0x4051800000000000,
        0x40589A9FEAA855DF,
        0x40524487F14699FC,
    ]


def test_all_shared_native_euler_cases_and_copied_coefficients():
    """All native scalar results share the pinned fixture's canonical bands."""
    fixture = Path(__file__).resolve().parents[3] / "sdk/go/testdata/geometric-brownian"
    cases = json.loads((fixture / "cases.json").read_text())["cases"]
    tolerances = json.loads((fixture / "oracle.json").read_text())["tolerances"]
    assert cases
    for case in cases:
        inputs = case["inputs"]
        process = GeometricBrownianMotionProcess(inputs["initial_value"], inputs["mu"], inputs["volatility"])
        assert process.mu() == inputs["mu"]
        assert process.volatility() == inputs["volatility"]
        t, state = inputs["time"], inputs["state"]
        actual = {
            "x0": process.x0(),
            "drift": process.drift(t, state),
            "diffusion": process.diffusion(t, state),
            "expectation": process.expectation(t, state, inputs["dt"]),
            "variance": process.variance(t, state, inputs["dt"]),
            "std_deviation": process.std_deviation(t, state, inputs["dt"]),
            "evolve": process.evolve(t, state, inputs["dt"], inputs["dw"]),
        }
        assert actual.keys() == case["native"].keys()
        for key, expected in case["native"].items():
            band = max(tolerances["scalar_absolute"], abs(expected) * tolerances["relative"])
            assert abs(actual[key] - expected) <= band, (case["name"], key, actual[key], expected)
