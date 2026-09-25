"""Finite-difference vanilla prices shared with the Go and Rust fixtures."""

import gc
import json
import math
import os
import subprocess
from pathlib import Path
from typing import Any, cast

import pytest

from itofin import Settings
from itofin.instruments import BermudanExercise, OptionType, VanillaOption
from itofin.pricingengines import FdBlackScholesVanillaEngine, FdScheme
from itofin.processes import BlackScholesProcess
from itofin.time import Date, DayCounter


def _market():
    today = Date(15, 1, 2025)
    expiry = Date(15, 1, 2026)
    settings = Settings()
    settings.set_evaluation_date(today)
    process = BlackScholesProcess(80.0, 0.05, 0.0, 0.25, today, DayCounter.actual365_fixed())
    return today, expiry, settings, process


@pytest.fixture(scope="module")
def core_oracle():
    oracle_path = os.environ.get("ITOFIN_FD_ORACLE_JSON")
    if oracle_path:
        source = Path(oracle_path).read_text()
    else:
        assert os.environ.get("GITHUB_ACTIONS") != "true", "CI must provide the same-profile Rust FD oracle"
        source = subprocess.check_output(
            ["cargo", "run", "--quiet", "--release", "-p", "libitofin", "--example", "fd_binding_oracle"],
            cwd=Path(__file__).resolve().parents[3],
            text=True,
        )
    oracle = json.loads(source)
    assert set(oracle) == {"european", "american", "bermudan"}
    for values in oracle.values():
        assert set(values) == {"npv", "delta", "gamma", "theta"}
        assert all(isinstance(value, (int, float)) and math.isfinite(value) for value in values.values())
    return oracle


@pytest.mark.parametrize(
    ("exercise", "expected"),
    [
        (
            "european",
            (18.266147644485358, -0.71491824907787493, 0.016981361087847299),
        ),
        (
            "american",
            (20.357667204554883, -0.85902979493468978, 0.026600330114210077),
        ),
        (
            "bermudan",
            (19.954434523211695, -0.81750545328763524, 0.019414167279763642),
        ),
    ],
)
def test_fd_core_and_go_prices_and_greeks(exercise, expected, core_oracle):
    """Stable Go values and same-profile Rust Greeks cover each exercise."""
    today, expiry, settings, process = _market()
    if exercise == "european":
        option = VanillaOption(OptionType.Put, 100.0, expiry, settings)
    elif exercise == "american":
        option = VanillaOption.american(OptionType.Put, 100.0, today, expiry, settings)
    else:
        dates = [Date(15, month, 2025) for month in (4, 7, 10)] + [expiry]
        option = VanillaOption.from_bermudan(OptionType.Put, 100.0, BermudanExercise(dates), settings)

    engine = FdBlackScholesVanillaEngine(process, t_grid=200, x_grid=200)
    option.set_fd_engine(engine)
    got = (option.npv(), option.delta(), option.gamma(), option.theta())
    assert got[:3] == pytest.approx(expected, rel=0, abs=1e-12)
    core = core_oracle[exercise]
    core_values = tuple(core[key] for key in ("npv", "delta", "gamma", "theta"))
    assert got == pytest.approx(core_values, rel=0, abs=1e-12)
    assert option.price_fd(engine) == pytest.approx(got[0], rel=0, abs=1e-12)

    del process, engine
    gc.collect()
    settings.set_evaluation_date(today + 1)
    assert math.isfinite(option.npv())


def test_fd_grid_validation_and_supported_schemes():
    """Reject invalid grids and restrict rollback to supported schemes."""
    _, expiry, settings, process = _market()
    for config in ({"t_grid": 0}, {"x_grid": 2}, {"t_grid": 2, "damping_steps": 2**64 - 1}):
        with pytest.raises((ValueError, OverflowError)):
            FdBlackScholesVanillaEngine(process, **cast(dict[str, Any], config))

    assert int(FdScheme.Douglas) == 0
    assert int(FdScheme.ImplicitEuler) == 1
    with pytest.raises(TypeError):
        FdBlackScholesVanillaEngine(process, scheme=cast(Any, 2))

    option = VanillaOption(OptionType.Put, 100.0, expiry, settings)
    default = option.price_fd(FdBlackScholesVanillaEngine(process))
    implicit = option.price_fd(FdBlackScholesVanillaEngine(process, scheme=FdScheme.ImplicitEuler))
    assert math.isfinite(default)
    assert math.isfinite(implicit)
    assert implicit != default
