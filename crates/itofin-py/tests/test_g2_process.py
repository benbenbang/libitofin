"""Exact bare-factor transitions, fixed shapes and corrected diffusion."""
import gc
import json
import math
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor

import pytest

from itofin import ItofinError
from itofin.processes import G2Process


def close(actual, expected):
    """Retain the native regular-reference relative tolerance."""
    assert abs(actual - expected) <= 8e-14 * max(abs(expected), 1e-300)


def test_parameters_shapes_ownership_and_apply():
    """Keep immutable parameters, fresh outputs and bare-factor-only scope."""
    p = G2Process(a=-0.1, sigma=0.03, b=0.4, eta=0.007, rho=-0.6)
    assert (p.size(), p.factors(), p.x0(), p.y0()) == (2, 2, 0, 0)
    assert (p.a(), p.sigma(), p.b(), p.eta(), p.rho()) == (-0.1, 0.03, 0.4, 0.007, -0.6)
    assert G2Process.__module__ == "itofin.processes"
    assert p.initial_values() == [0, 0]
    first = p.initial_values()
    first[0] = 99
    assert p.initial_values() == [0, 0]
    assert p.apply((1.0, -2.0), [3.0, 4.0]) == [4, 2]
    assert p.drift(1, [0.2, -0.3]) == [0.1 * 0.2, -0.4 * -0.3]
    matrix = p.diffusion(1, [0.2, -0.3])
    assert len(matrix) == 2 and all(len(row) == 2 for row in matrix)
    matrix[0][0] = 99
    assert p.diffusion(1, [0.2, -0.3])[0][0] == 0.03
    for name in ["time", "short_rate", "set_value", "tree", "phi"]:
        assert not hasattr(p, name)
    gc.collect()
    assert p.a() == -0.1


def test_regular_native_oracle_and_raw_diffusion_qualification():
    """Compare raw exact native transitions without copying diffusion defects."""
    source = Path(__file__).resolve().parents[3] / "sdk/go/testdata/g2_process.json"
    fixture = json.loads(source.read_text())
    assert "rho*sigma" in fixture["diffusion_qualification"]
    assert len(fixture["rows"]) == 4
    for r in fixture["rows"]:
        a, sigma, b, eta, rho = r["parameters"]
        p = G2Process(a, sigma, b, eta, rho)
        mean = p.expectation(r["t"], r["state"], r["dt"])
        cov = p.covariance(r["t"], r["state"], r["dt"])
        root = p.std_deviation(r["t"], r["state"], r["dt"])
        for i in range(2):
            close(mean[i], r["expectation"][i])
            for j in range(2):
                close(cov[i][j], r["covariance"][i][j])
                close(root[i][j], r["std_deviation"][i][j])
        diffusion = p.diffusion(r["t"], r["state"])
        close(r["raw_diffusion"][1][0], rho * sigma)
        close(diffusion[1][0], rho * eta)
        if sigma != eta and rho != 0:
            assert diffusion[1][0] != r["raw_diffusion"][1][0]


@pytest.mark.parametrize("a,b,rho", [(0, 0, 1), (0.3, 0.3, 1), (0.3, 0.7, -1), (-0.2, 0.2, 0.4)])
def test_analytic_integrals_gram_and_evolve(a, b, rho):
    """Check independent OU integrals and row-oriented Gaussian loadings."""
    sigma, eta, dt = 0.03, 0.007, 0.8
    p = G2Process(a, sigma, b, eta, rho)
    x, draws = [0.015, -0.007], [0.75, -0.4]

    def integral(k):
        return dt if k == 0 else -math.expm1(-k * dt) / k

    expected = [[sigma * sigma * integral(2 * a), rho * sigma * eta * integral(a + b)],
                [rho * sigma * eta * integral(a + b), eta * eta * integral(2 * b)]]
    mean = p.expectation(3, x, dt)
    close(mean[0], x[0] * math.exp(-a * dt))
    close(mean[1], x[1] * math.exp(-b * dt))
    root = p.std_deviation(3, x, dt)
    cov = p.covariance(3, x, dt)
    diffusion = p.diffusion(3, x)
    for i in range(2):
        for j in range(2):
            close(cov[i][j], expected[i][j])
            close(sum(root[i][k] * root[j][k] for k in range(2)), expected[i][j])
    close(sum(v * v for v in diffusion[1]), eta * eta)
    close(diffusion[0][0] * diffusion[1][0], rho * sigma * eta)
    next_state = p.evolve(3, x, dt, draws)
    for i in range(2):
        close(next_state[i], mean[i] + sum(root[i][j] * draws[j] for j in range(2)))
    assert p.evolve(3, x, 0, draws) == x


@pytest.mark.parametrize("sigma,eta", [(0, 0), (0, 0.007), (0.03, 0), (0.03, 0.007)])
def test_zero_reversions_volatilities_and_steps(sigma, eta):
    """Preserve deterministic factors and singular zero-step limits."""
    p = G2Process(0, sigma, 0, eta, -1)
    cov = p.covariance(0, [0, 0], 0.7)
    close(cov[0][0], sigma * sigma * 0.7)
    close(cov[1][1], eta * eta * 0.7)
    close(cov[0][1], -sigma * eta * 0.7)
    assert p.std_deviation(0, [0, 0], 0) == [[0, 0], [0, 0]]
    assert p.evolve(0, [0.1, -0.2], 0, [7, -3]) == [0.1, -0.2]


@pytest.mark.parametrize("a,b,rho,dt,expected", [
    (1e-18, 2e-18, 0.999999999999, 1, 1.1313583358943374e-8),
    (0.1, 0.2, 1, 1e-8, 2.3094010744491024e-16),
    (0.1, 0.1000001, -1, 1, 2.196404730112083e-10),
    (0, 0.02, 1, 1, 4.57290527571399e-5),
    (0.012, -0.006, -1, 1, 4.1693864126915885e-5),
])
def test_native_90_digit_decimal_residual_regressions(a, b, rho, dt, expected):
    """Retain the native strict independent-shock reference regressions."""
    p = G2Process(a, 0.01, b, 0.008, rho)
    actual = p.std_deviation(0, [0, 0], dt)[1][1]
    assert actual > 0
    assert abs(actual / expected - 1) < 2e-12
    assert p.evolve(0, [0, 0], dt, [0, 1])[1] == actual


@pytest.mark.parametrize("eta,dt,cross,loading", [
    (1e154, 800, 4.0000000000000003e-44, 2.0748631928110647e-191),
    (1e154, 740, 3.7e-44, 2.1917959166182907e-165),
    (1e-3, 800, 4e-201, 0),
])
@pytest.mark.parametrize("sign", [-1, 1])
def test_representable_cross_moments_survive_loading_underflow(eta, dt, cross, loading, sign):
    """Keep representable cross moments even when root entries underflow."""
    p = G2Process(-1, 1e-200, 1, eta, sign * 0.5)
    cov, root = p.covariance(0, [0, 0], dt), p.std_deviation(0, [0, 0], dt)
    assert abs(cov[0][1] / (sign * cross) - 1) < 2e-12
    assert cov[0][1] == cov[1][0]
    if loading == 0:
        assert root[1][0] == 0
        assert cov[0][1] != root[0][0] * root[1][0]
    else:
        assert abs(root[1][0] / (sign * loading) - 1) < 2e-12


def test_log_scaling_and_signed_reversion_extremes():
    """Preserve finite extreme transitions without avoidable overflow."""
    p = G2Process(1e200, 1e200, 1e200, 1e200, 0.5)
    cov = p.covariance(0, [0, 0], 1)
    close(cov[0][0], 5e199)
    close(cov[0][1], 2.5e199)
    p = G2Process(-1, 1e-200, -1, 1e-200, 0.5)
    mean = p.expectation(0, [1e-200, -1e-200], 800)
    close(mean[0], math.exp(800 + math.log(1e-200)))
    close(mean[1], -mean[0])


@pytest.mark.parametrize("index", range(5))
@pytest.mark.parametrize("bad", [math.nan, math.inf, -math.inf])
def test_reject_nonfinite_parameters(index, bad):
    """Reject nonfinite values in each scalar parameter position."""
    args = [0.1, 0.03, 0.4, 0.007, -0.6]
    args[index] = bad
    with pytest.raises(ItofinError):
        G2Process(*args)


@pytest.mark.parametrize("args", [(0, -1, 0, 0, 0), (0, 0, 0, -1, 0), (0, 0, 0, 0, 1.01), (0, 0, 0, 0, -1.01)])
def test_reject_constructor_domains(args):
    """Keep the native volatility and correlation domains."""
    with pytest.raises(ItofinError):
        G2Process(*args)


@pytest.mark.parametrize("x", [[], [0], [0, 0, 0], [math.nan, 0], [0, math.inf]])
def test_reject_all_malformed_state_and_noise_shapes(x):
    """Reject short, long and nonfinite state and noise inputs."""
    p = G2Process(0.1, 0.03, 0.4, 0.007, -0.6)
    for action in [lambda: p.drift(0, x), lambda: p.diffusion(0, x),
                   lambda: p.expectation(0, x, 1), lambda: p.std_deviation(0, x, 1),
                   lambda: p.covariance(0, x, 1), lambda: p.apply(x, [0, 0]),
                   lambda: p.apply([0, 0], x), lambda: p.evolve(0, [0, 0], 1, x)]:
        with pytest.raises(ItofinError, match="exactly two finite"):
            action()


@pytest.mark.parametrize("bad", [-1, math.nan, math.inf])
def test_reject_invalid_time_steps_and_endpoint(bad):
    """Reject invalid native times, steps and overflowing endpoints."""
    p = G2Process(0.1, 0.03, 0.4, 0.007, -0.6)
    for action in [lambda: p.drift(bad, [0, 0]), lambda: p.diffusion(bad, [0, 0]),
                   lambda: p.expectation(bad, [0, 0], 1), lambda: p.covariance(0, [0, 0], bad),
                   lambda: p.std_deviation(0, [0, 0], bad), lambda: p.evolve(0, [0, 0], bad, [0, 0])]:
        with pytest.raises(ItofinError):
            action()
    with pytest.raises(ItofinError):
        p.expectation(float.fromhex("0x1.fffffffffffffp+1023"), [0, 0], float.fromhex("0x1.fffffffffffffp+1023"))


def test_overflow_errors_do_not_poison_process_and_thread_confinement():
    """Recover after numerical errors and reject cross-thread access."""
    p = G2Process(-1e308, 0.03, 0.4, 0.007, -0.6)
    with pytest.raises(ItofinError):
        p.drift(0, [2, 0])
    with pytest.raises(ItofinError):
        p.apply([1e308, 0], [1e308, 0])
    with pytest.raises(ItofinError):
        p.expectation(0, [1, 0], 1)
    assert p.initial_values() == [0, 0]
    with ThreadPoolExecutor(max_workers=1) as executor:
        with pytest.raises(BaseException, match="unsendable|thread"):
            executor.submit(p.x0).result()
    assert p.x0() == 0
