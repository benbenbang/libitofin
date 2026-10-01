"""Seeded OU simulation parity and validation."""

from typing import Any

import numpy as np
import pytest

import itofin


CONFIG: dict[str, Any] = dict(initial=1.0, level=3.0, speed=0.5, volatility=0.2, horizon=1.0, steps=4, paths=3, seed=42)


def test_ou_seeded_paths_match_pinned_multistep_fixture():
    expected = np.array(
        [
            [1.0, 1.2049197140571397, 1.4938576175387845, 1.8262101587088548, 1.879255526298315],
            [1.0, 1.2932179159179402, 1.5663072780654845, 1.758274886469132, 1.9272460691202662],
            [1.0, 1.139911950142801, 1.3456668679224093, 1.4449524117278607, 1.50711446963386],
        ],
        dtype=np.float64,
    )
    full = itofin.simulate_ou(**CONFIG)
    np.testing.assert_allclose(full, expected, rtol=0.0, atol=2e-15)
    terminal = itofin.simulate_ou(**CONFIG, terminal_only=True)
    np.testing.assert_allclose(terminal, expected[:, -1], rtol=0.0, atol=2e-15)
    assert np.array_equal(terminal, full[:, -1])


def test_ou_seeded_first_transition_and_terminal_layout():
    full = itofin.simulate_ou(**CONFIG)
    assert full.shape == (3, 5)
    assert full.dtype == np.float64
    assert full.flags.c_contiguous
    assert np.array_equal(full[:, 0], [1.0] * 3)
    draw = itofin.gaussian_draws(1, 42)[0]
    dt = 0.25
    mean = 3.0 + (1.0 - 3.0) * np.exp(-0.5 * dt)
    std = np.sqrt(0.5 * 0.2 * 0.2 / 0.5 * (1.0 - np.exp(-2.0 * 0.5 * dt)))
    assert abs(full[0, 1] - (mean + std * draw)) < 1e-15
    terminal = itofin.simulate_ou(**CONFIG, terminal_only=True)
    assert terminal.shape == (3,)
    assert np.array_equal(terminal, full[:, -1])
    assert np.array_equal(full, itofin.simulate_ou(**CONFIG))


def test_ou_zero_horizon_and_invalid_inputs():
    assert np.array_equal(itofin.simulate_ou(**{**CONFIG, "horizon": 0}), np.ones((3, 5)))
    for replacement in ({"speed": -1.0}, {"volatility": -1.0}, {"seed": 0}, {"paths": 0}, {"initial": float("nan")}):
        with pytest.raises(itofin.ItofinError):
            itofin.simulate_ou(**{**CONFIG, **replacement})
    with pytest.raises(itofin.ItofinError, match="output limit"):
        itofin.simulate_ou(**CONFIG, max_output_values=2)
