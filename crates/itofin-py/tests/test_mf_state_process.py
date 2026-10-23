"""Standalone MF-state contracts, retained C++ observations and Decimal fixtures."""

import csv
import math
from pathlib import Path

import pytest

from itofin import ItofinError
from itofin.processes import MfStateProcess

DATA = Path(__file__).resolve().parents[3] / "sdk/go/testdata"


def rows(name):
    with (DATA / name).open() as f:
        return list(csv.DictReader(f, delimiter="\t"))


def near(got, want):
    assert abs(got - want) <= 2e-13 * max(abs(want), float.fromhex("0x1p-1022"))


def test_mf_state_parameters_buckets_and_owned_copies():
    times, vols = [0.0, 1.0, 2.0], [0.9, 0.2, 0.3, 0.4]
    p = MfStateProcess(0.7, times, vols)
    times[1], vols[1] = 9.0, 9.0
    assert p.reversion() == 0.7
    assert p.x0() == p.drift(2.0, 7.0) == 0.0
    assert p.times() == [0.0, 1.0, 2.0]
    assert p.volatilities() == [0.9, 0.2, 0.3, 0.4]
    p.times()[0], p.volatilities()[1] = 9.0, 9.0
    assert p.times()[0] == 0.0
    assert p.volatilities()[1] == 0.2
    for t, expected in [(0.0, 0.2), (0.999, 0.2), (1.0, 0.3), (2.0, 0.4), (10.0, 0.4)]:
        assert p.diffusion(t, -7.0) == expected
    assert p.expectation(0.5, -0.2, 2.0) == -0.2
    variance = p.variance(0.5, -0.2, 2.0)
    near(p.std_deviation(0.5, -0.2, 2.0), math.sqrt(variance))
    near(p.evolve(0.5, -0.2, 2.0, -0.7), -0.2 - 0.7 * math.sqrt(variance))
    assert p.evolve(0.0, 0.3, 0.0, 5.0) == 0.3


@pytest.mark.parametrize("a", [-0.4, 0.0, 0.4])
def test_mf_state_empty_grid_unit_variance_and_cutoffs(a):
    p = MfStateProcess(a, [], [0.0])
    assert p.times() == []
    assert p.diffusion(0.7, 0.0) == 0.0
    expected = 0.2 if a == 0.0 else math.exp(2 * a * 0.7) * math.expm1(2 * a * 0.2) / (2 * a)
    near(p.variance(0.7, 0.0, 0.2), expected)
    assert p.variance(0.0, 0.0, math.ulp(1.0) / 2) == 0.0
    assert p.variance(0.0, 0.0, math.ulp(1.0)) > 0.0


@pytest.mark.parametrize("r", rows("mf_state_native.tsv"))
def test_mf_state_native_observations(r):
    times, vols = ([0.5, 1.5], [0.2, 0.4, 0.1]) if r["empty"] == "0" else ([], [0.2])
    p = MfStateProcess(float(r["a"]), times, vols)
    t, dt = float(r["t0"]), float(r["dt"])
    near(p.diffusion(t, 3.0), float(r["diffusion"]))
    near(p.variance(t, 3.0, dt), float(r["variance"]))
    near(p.std_deviation(t, 3.0, dt), float(r["stddev"]))


@pytest.mark.parametrize("r", rows("mf_state_reference.tsv"))
def test_mf_state_high_precision_references(r):
    array = lambda value: [] if value == "-" else [float(x) for x in value.split(";")]
    p = MfStateProcess(float(r["a"]), array(r["times"]), array(r["vols"]))
    t, dt = float(r["t0"]), float(r["dt"])
    near(p.variance(t, 3.0, dt), float(r["variance"]))
    near(p.std_deviation(t, -2.0, dt), float(r["stddev"]))
    near(p.evolve(t, 0.0, dt, 1.0), float(r["stddev"]))
    assert p.expectation(t, -2.0, dt) == -2.0


@pytest.mark.parametrize("args", [
    (math.nan, [], [0.2]), (math.inf, [], [0.2]), (-math.inf, [], [0.2]),
    (0.0, [], []), (0.0, [1.0], [0.2]), (0.0, [1.0, 1.0], [0.2] * 3),
    (0.0, [2.0, 1.0], [0.2] * 3), (0.0, [-1.0], [0.2] * 2),
    (0.0, [math.nan], [0.2] * 2), (0.0, [math.inf], [0.2] * 2),
    (0.0, [], [-0.2]), (0.0, [], [math.inf]), (0.0, [], [math.nan]),
])
def test_mf_state_invalid_parameters(args):
    with pytest.raises(ItofinError):
        MfStateProcess(*args)


def test_mf_state_invalid_transitions():
    p = MfStateProcess(0.1, [1.0], [0.2, 0.3])
    for call in [lambda: p.drift(-1.0, 0.0), lambda: p.diffusion(0.0, math.nan),
                 lambda: p.expectation(0.0, 0.0, -1.0),
                 lambda: p.std_deviation(0.0, math.inf, 1.0),
                 lambda: p.variance(1e308, 0.0, 1e308),
                 lambda: p.evolve(0.0, 0.0, 1.0, math.nan)]:
        with pytest.raises(ItofinError):
            call()
    assert p.x0() == 0.0


def test_mf_state_is_unsendable():
    import threading

    p = MfStateProcess(0.0, [1.0], [0.2, 0.3])
    errors = []

    def foreign_thread():
        try:
            p.x0()
        except BaseException as error:
            errors.append(str(error))

    worker = threading.Thread(target=foreign_thread)
    worker.start()
    worker.join()
    assert len(errors) == 1
    assert "unsendable" in errors[0] or "thread" in errors[0]
    assert p.x0() == 0.0
