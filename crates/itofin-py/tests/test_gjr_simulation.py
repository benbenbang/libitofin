"""Seeded GJR-GARCH path layouts, budgets and independent QuantLib paths."""

import gc
import json
import math
import sys
from pathlib import Path
from typing import Any

import numpy as np
import pytest

import itofin

CONFIG: dict[str, Any] = dict(
    spot=100.0,
    daily_variance=0.04 / 252,
    risk_free_rate=0.03,
    dividend_yield=0.01,
    omega=2e-6,
    alpha=0.04,
    beta=0.88,
    gamma=0.08,
    lambda_=-0.4,
    horizon=0.1,
    steps=8,
    paths=3,
    seed=42,
)
SCHEMES = ("PartialTruncation", "FullTruncation", "Reflection")
FIXTURES = Path(__file__).resolve().parents[3] / "sdk/go/testdata"


def bits(values):
    """Bits."""
    return np.asarray(values, dtype=np.float64).view(np.uint64)


@pytest.mark.parametrize("scheme", SCHEMES)
def test_layout_dtype_draw_reproducibility_and_terminal_bits(scheme):
    """Layout dtype draw reproducibility and terminal bits."""
    full = itofin.simulate_gjr(**CONFIG, scheme=scheme)
    terminal = itofin.simulate_gjr(**CONFIG, scheme=scheme, terminal_only=True)
    assert full.shape == (3, 9, 2)
    assert terminal.shape == (3, 2)
    assert full.dtype == terminal.dtype == np.float64
    assert full.flags.c_contiguous and terminal.flags.c_contiguous
    np.testing.assert_array_equal(full[:, 0, :], [[100.0, 0.04]] * 3)
    np.testing.assert_array_equal(bits(terminal), bits(full[:, -1, :]))
    np.testing.assert_array_equal(bits(full), bits(itofin.simulate_gjr(**CONFIG, scheme=scheme)))
    different = itofin.simulate_gjr(**{**CONFIG, "seed": 43}, scheme=scheme)
    assert not np.array_equal(bits(full[:, 1:, :]), bits(different[:, 1:, :]))


@pytest.mark.parametrize("scheme", SCHEMES)
@pytest.mark.parametrize("seed", [1, 42, 2**32 - 1])
def test_zero_horizon_exact_initial_state(scheme, seed):
    """Zero horizon exact initial state."""
    config = {**CONFIG, "horizon": 0.0, "seed": seed}
    full = itofin.simulate_gjr(**config, scheme=scheme)
    terminal = itofin.simulate_gjr(**config, scheme=scheme, terminal_only=True)
    np.testing.assert_array_equal(bits(full), bits(np.tile([100.0, 0.04], (3, 9, 1))))
    np.testing.assert_array_equal(bits(terminal), bits(full[:, -1, :]))


def test_output_owned_and_mutable_without_affecting_subsequent_calls():
    """Output owned and mutable without affecting subsequent calls."""
    full = itofin.simulate_gjr(**CONFIG)
    saved = full.copy()
    terminal = itofin.simulate_gjr(**CONFIG, terminal_only=True)
    expected_terminal = terminal.copy()
    gc.collect()
    with pytest.raises(itofin.ItofinError):
        itofin.simulate_gjr(**{**CONFIG, "spot": 0.0})
    itofin.simulate_gjr(**{**CONFIG, "seed": 100})
    np.testing.assert_array_equal(bits(full), bits(saved))
    np.testing.assert_array_equal(bits(terminal), bits(expected_terminal))
    full[0, 1, 0] = -123.0
    np.testing.assert_array_equal(bits(itofin.simulate_gjr(**CONFIG)), bits(saved))
    np.testing.assert_array_equal(bits(terminal), bits(expected_terminal))


@pytest.mark.parametrize(
    "key,value",
    [
        ("spot", 0.0),
        ("spot", -1.0),
        ("daily_variance", -0.01),
        ("omega", -1.0),
        ("alpha", -0.01),
        ("beta", -0.01),
        ("gamma", -0.1),
        ("days_per_year", 0.0),
        ("horizon", -0.1),
        ("steps", 0),
        ("paths", 0),
        ("seed", 0),
        *[
            (key, value)
            for key in (
                "spot",
                "daily_variance",
                "risk_free_rate",
                "dividend_yield",
                "omega",
                "alpha",
                "beta",
                "gamma",
                "lambda_",
                "days_per_year",
                "horizon",
            )
            for value in (math.nan, math.inf)
        ],
    ],
)
def test_invalid_domains_rejected(key, value):
    """Invalid domains rejected."""
    with pytest.raises(itofin.ItofinError):
        itofin.simulate_gjr(**{**CONFIG, key: value})


@pytest.mark.parametrize(
    "key,value",
    [
        ("steps", -1),
        ("paths", -1),
        ("seed", -1),
        ("seed", 2**32),
        ("steps", 2**64),
        ("paths", 2**64),
        ("max_output_values", sys.maxsize + 1),
    ],
)
def test_integer_overflow_rejected(key, value):
    """Integer overflow rejected."""
    with pytest.raises(OverflowError):
        itofin.simulate_gjr(**{**CONFIG, key: value})


@pytest.mark.parametrize("key", ["steps", "paths", "seed", "max_output_values"])
def test_noninteger_arguments_rejected(key):
    """Noninteger arguments rejected."""
    with pytest.raises(TypeError):
        itofin.simulate_gjr(**{**CONFIG, key: 1.5})


@pytest.mark.parametrize("scheme", ["", "full", "fulltruncation", "qem"])
def test_unknown_scheme_rejected(scheme):
    """Unknown scheme rejected."""
    with pytest.raises(itofin.ItofinError):
        itofin.simulate_gjr(**CONFIG, scheme=scheme)


@pytest.mark.parametrize("terminal", [False, True])
def test_exact_budget_boundary_and_overflow(terminal):
    """Exact budget boundary and overflow."""
    count = CONFIG["paths"] * 2 * (1 if terminal else CONFIG["steps"] + 1)
    with pytest.raises(itofin.ItofinError):
        itofin.simulate_gjr(**CONFIG, terminal_only=terminal, max_output_values=count - 1)
    itofin.simulate_gjr(**CONFIG, terminal_only=terminal, max_output_values=count)
    with pytest.raises(itofin.ItofinError):
        itofin.simulate_gjr(**CONFIG, terminal_only=terminal, max_output_values=-1)
    with pytest.raises(itofin.ItofinError):
        itofin.simulate_gjr(**{**CONFIG, "steps": sys.maxsize, "paths": sys.maxsize}, terminal_only=terminal)


PATH_CASES = json.loads((FIXTURES / "gjrgarch-paths.json").read_text())["cases"]


@pytest.mark.parametrize("case", PATH_CASES, ids=lambda case: case["name"])
def test_independent_quantlib_seeded_paths(case):
    """Independent quantlib seeded paths."""
    config = dict(case["input"])
    config["scheme"] = SCHEMES[config.pop("discretization")]
    config["lambda_"] = config.pop("lambda")
    config.pop("terminal_only", None)
    full = itofin.simulate_gjr(**config)
    terminal = itofin.simulate_gjr(**config, terminal_only=True)
    np.testing.assert_allclose(full.ravel(), case["expected_full"], rtol=2e-13, atol=1e-12)
    np.testing.assert_allclose(terminal.ravel(), case["expected_terminal"], rtol=2e-13, atol=1e-12)
    np.testing.assert_array_equal(bits(terminal), bits(full[:, -1, :]))
