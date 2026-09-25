"""Shared-kernel bit and error parity with sdk/go/simulation_test.go."""

from typing import Any

import numpy as np
import pytest

import itofin


CONFIG: dict[str, Any] = dict(
    initial=[100.0, 70.0],
    drift=[0.05, -0.02],
    volatility=[0.2, 0.3],
    correlation=[1.0, 0.5, 0.5, 1.0],
    horizon=1.0,
    steps=12,
    paths=100,
    seed=42,
)


def test_gaussian_draws_match_go_bits():
    """The shared Gaussian stream retains the Go fixture's float64 bits."""
    values = itofin.gaussian_draws(100, 42)
    assert values.dtype == np.float64
    assert values.shape == (100,)
    assert values[:6].view(np.uint64).tolist() == [
        0xBFD478762AC42BAF,
        0x3FEA89ECAA3CA273,
        0x3FFA6DDA2828B3F9,
        0xBFECE0129FC56C99,
        0x3FE3CDA83F040D7C,
        0x3FE8AD42D74057E8,
    ]
    assert np.array_equal(values, itofin.gaussian_draws(100, 42))
    assert itofin.gaussian_draws(0, 42).shape == (0,)
    with pytest.raises(itofin.ItofinError, match="seed must be nonzero"):
        itofin.gaussian_draws(1, 0)
    with pytest.raises(itofin.ItofinError, match="invalid draw count"):
        itofin.gaussian_draws(itofin.DEFAULT_MAX_OUTPUT_VALUES + 1, 42)


def test_gbm_matches_go_bits_layout_and_terminal_values():
    """Seeded paths match Go exactly and terminal mode preserves final rows."""
    full = itofin.simulate_gbm(**CONFIG)
    assert full.shape == (100, 13, 2)
    assert full.dtype == np.float64
    assert full.flags.c_contiguous
    assert full.ravel()[:12].view(np.uint64).tolist() == [
        0x4059000000000000,
        0x4051800000000000,
        0x40589A9FEAA855DF,
        0x40524487F14699FC,
        0x405B223B02E57074,
        0x40523D3520F9416D,
        0x405C30EC5EE1B1AF,
        0x4053BE21D0059094,
        0x405CAC1F016D9A59,
        0x405437D45B70E53E,
        0x405B1D4A6F41824D,
        0x40530D4DA919E6C0,
    ]
    assert np.array_equal(full[:, 0, :], np.array([CONFIG["initial"]] * 100))
    terminal = itofin.simulate_gbm(**CONFIG, terminal_only=True)
    assert terminal.shape == (100, 2)
    assert np.array_equal(terminal, full[:, -1, :])
    assert np.array_equal(full, itofin.simulate_gbm(**CONFIG))


def test_gbm_default_identity_zero_time_and_error_parity():
    """The convenience inputs and invalid cases follow the Go contract."""
    scalar = {**CONFIG, "initial": [100.0], "drift": [0.05], "volatility": [0.2], "correlation": None}
    identity = itofin.simulate_gbm(**scalar)
    assert np.array_equal(identity, itofin.simulate_gbm(**{**scalar, "correlation": [1.0]}))
    arrays = {**scalar, "initial": np.array([100.0]), "drift": np.array([0.05]), "volatility": np.array([0.2])}
    assert np.array_equal(identity, itofin.simulate_gbm(**arrays))
    zero_time = itofin.simulate_gbm(**{**scalar, "horizon": 0.0})
    assert np.array_equal(zero_time, np.full((100, 13, 1), 100.0))
    with pytest.raises(itofin.ItofinError, match="seed must be nonzero"):
        itofin.simulate_gbm(**{**CONFIG, "seed": 0})
    with pytest.raises(itofin.ItofinError, match="positive definite"):
        itofin.simulate_gbm(**{**CONFIG, "correlation": [1.0, 1.0, 1.0, 1.0]})
    with pytest.raises(itofin.ItofinError, match="output limit"):
        itofin.simulate_gbm(**CONFIG, max_output_values=2)
    with pytest.raises(itofin.ItofinError, match="max_output_values"):
        itofin.simulate_gbm(**CONFIG, max_output_values=-1)
    assert itofin.simulate_gbm(**CONFIG, max_output_values=2600).shape == (100, 13, 2)
    assert itofin.simulate_gbm(**CONFIG, terminal_only=True, max_output_values=200).shape == (100, 2)
