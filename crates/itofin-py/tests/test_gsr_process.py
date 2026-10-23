"""Standalone GSR contracts with independently derived zero-reversion limits."""

import math

import pytest

from itofin import ItofinError
from itofin.processes import GsrProcess


@pytest.mark.parametrize("a", [0.0, 1e-15, 0.2, -0.2])
def test_gsr_constant_grid_and_transitions(a):
    p = GsrProcess([1.0, 2.0], [0.03] * 3, [a] * 3, 10.0)
    assert p.x0() == 0.0
    assert p.forward_measure_time() == 10.0
    assert p.sigma(10.0) == 0.03
    assert p.reversion(0.0) == a
    t, x, dt, dw = 2.0, 0.1, 0.8, -0.7
    kernel = lambda k, d: d if k == 0 else -math.expm1(-k * d) / k
    assert p.g(t, t + dt) == pytest.approx(kernel(a, dt), rel=2e-14)
    assert p.y(t) == pytest.approx(0.03**2 * kernel(2 * a, t), rel=2e-14)
    assert p.drift(t, x) == pytest.approx(p.y(t) - 0.03**2 * p.g(t, 10.0) - a * x)
    assert p.diffusion(t, x) == 0.03
    variance = 0.03**2 * kernel(2 * a, dt)
    assert p.variance(t, x, dt) == pytest.approx(variance, rel=2e-14)
    assert p.std_deviation(t, x, dt) ** 2 == pytest.approx(variance, rel=2e-14)
    assert p.evolve(t, x, dt, dw) == pytest.approx(p.expectation(t, x, dt) + math.sqrt(variance) * dw)
    assert p.expectation(t, x, 0.0) == x
    assert p.evolve(t, x, 0.0, dw) == x
    if a == 0:
        expected = x + 0.03**2 * (t * dt - dt * (10.0 - t - dt))
        assert p.expectation(t, x, dt) == pytest.approx(expected, rel=2e-14)


def test_gsr_horizon_mutation_and_deterministic_limit():
    p = GsrProcess([2.0], [0.03, 0.03], [0.0], 10.0)
    old = p.expectation(0.0, 0.1, 1.0)
    p.set_forward_measure_time(5.0)
    assert p.expectation(0.0, 0.1, 1.0) != old
    fresh = GsrProcess([], [0.03], [0.0], 5.0)
    assert p.expectation(0.0, 0.1, 1.0) == fresh.expectation(0.0, 0.1, 1.0)
    for bad in [-1.0, math.nan, math.inf]:
        with pytest.raises(ItofinError):
            p.set_forward_measure_time(bad)
        assert p.forward_measure_time() == 5.0
    p.set_forward_measure_time(0.0)
    assert p.g(0.0, 0.0) == 0.0
    assert p.evolve(0.0, 0.3, 0.0, 2.0) == 0.3
    q = GsrProcess([], [0.0], [-0.2], 10.0)
    assert q.expectation(0.0, 0.3, 2.0) == pytest.approx(0.3 * math.exp(0.4))
    assert q.variance(0.0, 0.3, 2.0) == 0.0


@pytest.mark.parametrize("args", [
    ([], [], [], 10.0), ([1.0], [0.03], [0.1], 10.0),
    ([1.0], [0.03, 0.04], [0.1], 10.0),
    ([1.0], [0.03, 0.03], [0.1, 0.2], 10.0),
    ([1.0, 1.0], [0.03]*3, [0.1], 10.0),
    ([-1.0], [0.03]*2, [0.1], 10.0),
    ([], [-0.03], [0.1], 10.0), ([], [0.03], [math.nan], 10.0),
    ([], [0.03], [0.1], -1.0), ([], [math.inf], [0.1], 10.0),
])
def test_gsr_rejects_invalid_grids(args):
    with pytest.raises(ItofinError):
        GsrProcess(*args)


def test_gsr_invalid_queries_and_copy():
    vols = [0.03]
    p = GsrProcess([], vols, [0.1], 2.0)
    vols[0] = 0.9
    assert p.sigma(0.0) == 0.03
    for query in [lambda: p.sigma(2.1), lambda: p.reversion(-1.0),
                  lambda: p.y(math.nan), lambda: p.g(1.0, 0.5),
                  lambda: p.drift(0.0, math.inf), lambda: p.diffusion(-1.0, 0.0),
                  lambda: p.expectation(1.5, 0.0, 0.6),
                  lambda: p.std_deviation(0.0, math.nan, 1.0),
                  lambda: p.variance(0.0, 0.0, -1.0),
                  lambda: p.evolve(0.0, 0.0, 1.0, math.nan)]:
        with pytest.raises(ItofinError):
            query()
