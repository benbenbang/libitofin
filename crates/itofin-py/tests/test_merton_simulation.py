"""Seeded Merton paths, output ownership and numerical error contracts."""

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
    drift=.03,
    volatility=.2,
    jump_intensity=.7,
    log_mean_jump=-.1,
    log_jump_volatility=.25,
    horizon=1.0,
    steps=4,
    paths=3,
    seed=42,
)


def bits(values):
    return np.asarray(values, dtype=np.float64).view(np.uint64)


def test_merton_shapes_contiguity_reproducibility_and_terminal_bits():
    full = itofin.simulate_merton(**CONFIG)
    terminal = itofin.simulate_merton(**CONFIG, terminal_only=True)
    assert full.shape == (3, 5)
    assert terminal.shape == (3,)
    assert full.dtype == terminal.dtype == np.float64
    assert full.flags.c_contiguous and terminal.flags.c_contiguous
    np.testing.assert_array_equal(full[:, 0], [CONFIG["spot"]] * CONFIG["paths"])
    np.testing.assert_array_equal(bits(terminal), bits(full[:, -1]))
    np.testing.assert_array_equal(bits(full), bits(itofin.simulate_merton(**CONFIG)))
    different = itofin.simulate_merton(**{**CONFIG, "seed": 43})
    assert not np.array_equal(bits(full[:, 1:]), bits(different[:, 1:]))


@pytest.mark.parametrize("seed", [1, 42, 2**32 - 1])
@pytest.mark.parametrize("volatility", [0.0, .2])
@pytest.mark.parametrize("horizon", [0.0, 1.0])
def test_zero_jump_matches_scalar_gbm_bits(seed, volatility, horizon):
    config = {**CONFIG, "jump_intensity": 0.0, "seed": seed, "volatility": volatility, "horizon": horizon}
    full = itofin.simulate_merton(**config)
    gbm = itofin.simulate_gbm(
        [config["spot"]], [config["drift"]], [volatility], horizon,
        config["steps"], config["paths"], seed,
    )
    np.testing.assert_array_equal(bits(full), bits(gbm[:, :, 0]))
    terminal = itofin.simulate_merton(**config, terminal_only=True)
    gbm_terminal = itofin.simulate_gbm(
        [config["spot"]], [config["drift"]], [volatility], horizon,
        config["steps"], config["paths"], seed, terminal_only=True,
    )
    np.testing.assert_array_equal(bits(terminal), bits(gbm_terminal[:, 0]))


def test_zero_horizon_repeats_initial_spot_exactly():
    config = {**CONFIG, "horizon": 0.0, "spot": .1}
    full = itofin.simulate_merton(**config)
    terminal = itofin.simulate_merton(**config, terminal_only=True)
    np.testing.assert_array_equal(bits(full), bits(np.full((3, 5), .1)))
    np.testing.assert_array_equal(bits(terminal), bits(np.full(3, .1)))


def test_outputs_remain_owned_after_other_calls_errors_and_garbage_collection():
    config = dict(CONFIG)
    full = itofin.simulate_merton(**config)
    saved = full.copy()
    terminal = itofin.simulate_merton(**config, terminal_only=True)
    saved_terminal = terminal.copy()
    del config
    gc.collect()
    itofin.simulate_merton(**{**CONFIG, "seed": 100})
    with pytest.raises(itofin.ItofinError):
        itofin.simulate_merton(**{**CONFIG, "spot": 0})
    np.testing.assert_array_equal(bits(full), bits(saved))
    np.testing.assert_array_equal(bits(terminal), bits(saved_terminal))
    full[0, 1] = -123.0
    np.testing.assert_array_equal(bits(terminal), bits(saved_terminal))
    np.testing.assert_array_equal(bits(itofin.simulate_merton(**CONFIG)), bits(saved))


INVALID_DOMAINS = [
    ("spot", 0.0), ("spot", -1.0), ("spot", math.nan), ("spot", math.inf),
    ("drift", math.nan), ("drift", math.inf), ("drift", -math.inf),
    ("volatility", -.1), ("volatility", math.nan), ("volatility", math.inf),
    ("jump_intensity", -.1), ("jump_intensity", math.nan), ("jump_intensity", math.inf),
    ("log_mean_jump", math.nan), ("log_mean_jump", math.inf), ("log_mean_jump", -math.inf),
    ("log_jump_volatility", -.1), ("log_jump_volatility", math.nan), ("log_jump_volatility", math.inf),
    ("horizon", -.1), ("horizon", math.nan), ("horizon", math.inf),
    ("steps", 0), ("paths", 0), ("seed", 0),
]


@pytest.mark.parametrize("name,value", INVALID_DOMAINS)
def test_invalid_domain_errors_and_subsequent_calls_recover(name, value):
    with pytest.raises(itofin.ItofinError):
        itofin.simulate_merton(**{**CONFIG, name: value})
    np.testing.assert_array_equal(
        bits(itofin.simulate_merton(**CONFIG, terminal_only=True)),
        bits(itofin.simulate_merton(**CONFIG)[:, -1]),
    )


@pytest.mark.parametrize("name,value", [("steps", -1), ("paths", -1), ("seed", -1), ("seed", 2**32), ("steps", 2**64), ("paths", 2**64), ("max_output_values", sys.maxsize + 1)])
def test_integer_binding_ranges_are_checked(name, value):
    with pytest.raises(OverflowError):
        itofin.simulate_merton(**{**CONFIG, name: value})


@pytest.mark.parametrize("name", ["steps", "paths", "seed", "max_output_values"])
def test_noninteger_counts_and_seed_are_rejected(name):
    with pytest.raises(TypeError):
        itofin.simulate_merton(**{**CONFIG, name: 1.5})


@pytest.mark.parametrize("changes", [
    {"volatility": 1e200},
    {"log_jump_volatility": 1e200},
    {"log_mean_jump": 1000.0},
    {"log_mean_jump": -1000.0},
    {"horizon": float.fromhex("0x0.0000000000001p-1022"), "steps": 2},
    {"jump_intensity": float.fromhex("0x0.0000000000001p-1022"), "horizon": .5, "steps": 1},
    {"jump_intensity": 709.0, "horizon": 1.0, "steps": 1},
    {"jump_intensity": 1e308, "horizon": 10.0, "steps": 1},
])
def test_unrepresentable_parameters_and_poisson_domain_are_errors(changes):
    with pytest.raises(itofin.ItofinError):
        itofin.simulate_merton(**{**CONFIG, **changes})


@pytest.mark.parametrize("terminal_only", [False, True])
@pytest.mark.parametrize("drift", [1000.0, -1000.0])
def test_midpath_spot_overflow_or_underflow_is_an_error(terminal_only, drift):
    config = {**CONFIG, "spot": 1.0, "drift": drift, "volatility": 0.0, "jump_intensity": 0.0}
    with pytest.raises(itofin.ItofinError):
        itofin.simulate_merton(**config, terminal_only=terminal_only)
    np.testing.assert_array_equal(bits(itofin.simulate_merton(**CONFIG)), bits(itofin.simulate_merton(**CONFIG)))


def test_output_limits_include_initial_column_and_terminal_mode_is_smaller():
    config = {**CONFIG, "steps": 3, "paths": 2}
    with pytest.raises(itofin.ItofinError, match="output limit"):
        itofin.simulate_merton(**config, max_output_values=7)
    assert itofin.simulate_merton(**config, max_output_values=8).shape == (2, 4)
    with pytest.raises(itofin.ItofinError, match="output limit"):
        itofin.simulate_merton(**config, terminal_only=True, max_output_values=1)
    assert itofin.simulate_merton(**config, terminal_only=True, max_output_values=2).shape == (2,)
    with pytest.raises(itofin.ItofinError, match="nonnegative"):
        itofin.simulate_merton(**config, max_output_values=-1)
    with pytest.raises(itofin.ItofinError, match="output limit"):
        itofin.simulate_merton(**{**CONFIG, "steps": 2**24, "paths": 1})


def test_output_and_three_draw_work_dimensions_are_checked_before_allocation():
    usize_max = 2 * sys.maxsize + 1
    for config in (
        {**CONFIG, "steps": usize_max, "paths": 1},
        {**CONFIG, "steps": usize_max // 2, "paths": 2, "terminal_only": True},
        {**CONFIG, "steps": 1, "paths": usize_max},
    ):
        with pytest.raises(itofin.ItofinError):
            itofin.simulate_merton(**config)


def test_independent_seeded_fixtures_cover_every_full_and_terminal_value():
    fixture_dir = Path(__file__).resolve().parents[3] / "sdk/go/testdata"
    cases = []
    for filename in ("merton-paths-oracle.json", "merton-paths-additional.json"):
        fixture = json.loads((fixture_dir / filename).read_text())
        assert len(fixture["cases"]) == 6
        cases.extend(fixture["cases"])
    assert len(cases) == 12
    for case in cases:
        config = dict(case["input"])
        config["terminal_only"] = False
        expected = np.asarray(case["expected_full"], dtype=np.float64).reshape(config["paths"], config["steps"] + 1)
        expected_terminal = np.asarray(case["expected_terminal"], dtype=np.float64)
        full = itofin.simulate_merton(**config)
        config["terminal_only"] = True
        terminal = itofin.simulate_merton(**config)
        assert np.all(np.abs(full - expected) <= 2e-14 * np.maximum(1.0, np.abs(expected))), case["name"]
        assert np.all(np.abs(terminal - expected_terminal) <= 2e-14 * np.maximum(1.0, np.abs(expected_terminal))), case["name"]
        np.testing.assert_array_equal(bits(terminal), bits(full[:, -1]))
