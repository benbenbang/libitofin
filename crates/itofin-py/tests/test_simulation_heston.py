"""Seeded Heston path facade and layout checks."""

from typing import Any

import numpy as np
import pytest

import itofin


CONFIG: dict[str, Any] = dict(
    spot=100.0,
    variance=0.04,
    risk_free_rate=0.05,
    dividend_yield=0.02,
    kappa=1.2,
    theta=0.06,
    sigma=0.3,
    rho=-0.5,
    horizon=1.0,
    steps=4,
    paths=3,
    seed=42,
    scheme="qem",
)


def test_heston_shape_terminal_seed_and_zero_horizon():
    full = itofin.simulate_heston(**CONFIG)
    assert full.shape == (3, 5, 2)
    assert full.dtype == np.float64
    assert full.flags.c_contiguous
    assert np.array_equal(full[:, 0], [[100.0, 0.04]] * 3)
    assert full[0, 1, 0] == pytest.approx(93.21873664131503, rel=0.0, abs=1e-11)
    assert full[0, 1, 1] == pytest.approx(0.06567515852602028, rel=0.0, abs=1e-13)
    assert np.array_equal(full, itofin.simulate_heston(**CONFIG))
    terminal = itofin.simulate_heston(**CONFIG, terminal_only=True)
    assert terminal.shape == (3, 2)
    assert np.array_equal(terminal, full[:, -1])
    zero = itofin.simulate_heston(**{**CONFIG, "horizon": 0.0})
    assert np.array_equal(zero, np.tile([100.0, 0.04], (3, 5, 1)))


def test_heston_scheme_and_errors():
    qem = itofin.simulate_heston(**CONFIG)
    qe = itofin.simulate_heston(**{**CONFIG, "scheme": "qe"})
    assert not np.array_equal(qem, qe)
    high = itofin.simulate_heston(
        **{
            **CONFIG,
            "variance": 0.01,
            "kappa": 0.5,
            "theta": 0.01,
            "sigma": 0.2,
            "steps": 1,
            "paths": 1,
            "scheme": "qe",
            "terminal_only": True,
        }
    )
    assert high[0, 0] == pytest.approx(96.55317400157244, rel=0.0, abs=1e-11)
    assert high[0, 1] == pytest.approx(0.018076059597846472, rel=0.0, abs=1e-13)
    with pytest.raises(itofin.ItofinError, match="discretization"):
        itofin.simulate_heston(**{**CONFIG, "scheme": "full_truncation"})
    for key, value in [
        ("seed", 0),
        ("spot", 0.0),
        ("variance", -0.1),
        ("theta", 0.0),
        ("sigma", np.inf),
        ("rho", 1.1),
        ("horizon", -1.0),
        ("steps", 0),
    ]:
        with pytest.raises(itofin.ItofinError):
            itofin.simulate_heston(**{**CONFIG, key: value})
    with pytest.raises(itofin.ItofinError, match="output limit"):
        itofin.simulate_heston(**CONFIG, max_output_values=2)
