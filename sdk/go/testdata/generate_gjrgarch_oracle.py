"""Generate GJR fixtures with QuantLib 1.43, never libitofin."""

import argparse
import json
import math
import statistics
from pathlib import Path

import QuantLib as ql

from generate_merton_paths_oracle import MT19937, normal

COMMIT = "9863b578af0caa4cecabf697196533e84a8308b6"
BASE = dict(
    spot=100.0,
    daily_variance=0.04 / 252,
    risk_free_rate=0.03,
    dividend_yield=0.01,
    omega=0.000002,
    alpha=0.04,
    beta=0.88,
    gamma=0.08,
    lambda_parameter=-0.4,
    days_per_year=252.0,
    discretization=1,
)


def process(config):
    date = ql.Date(15, 1, 2026)
    dc = ql.Actual365Fixed()
    rf = ql.YieldTermStructureHandle(ql.FlatForward(date, config["risk_free_rate"], dc))
    div = ql.YieldTermStructureHandle(ql.FlatForward(date, config["dividend_yield"], dc))
    spot = ql.QuoteHandle(ql.SimpleQuote(config["spot"]))
    return ql.GJRGARCHProcess(
        rf, div, spot, config["daily_variance"], config["omega"],
        config["alpha"], config["beta"], config["gamma"],
        config["lambda_parameter"], config["days_per_year"], config["discretization"],
    )


def parameters(config):
    return {("lambda" if k == "lambda_parameter" else k): v for k, v in config.items()}


def coefficients(config):
    lam = config["lambda_parameter"]
    cdf = math.erfc(-lam / math.sqrt(2)) / 2
    pdf = math.exp(-lam * lam / 2) / math.sqrt(2 * math.pi)
    q2 = 1 + lam * lam
    q3 = lam * pdf + cdf + lam * lam * cdf
    sigma2 = 2 + 4 * lam * lam
    fourth = lam**3 * pdf + 5 * lam * pdf + 3 * cdf + lam**4 * cdf + 6 * lam * lam * cdf
    sigma3 = fourth - q3 * q3
    sigma12 = -2 * lam
    sigma13 = -2 * pdf - 2 * lam * cdf
    sigma23 = 2 * cdf + sigma12 * sigma13
    alpha, gamma, days = config["alpha"], config["gamma"], config["days_per_year"]
    residual = (
        alpha * alpha * (sigma2 - sigma12 * sigma12)
        + gamma * gamma * (sigma3 - sigma13 * sigma13)
        + 2 * alpha * gamma * (sigma23 - sigma12 * sigma13)
    )
    return dict(
        persistence=config["beta"] + alpha * q2 + gamma * q3,
        variance_loading_0=math.sqrt(days) * (alpha * sigma12 + gamma * sigma13),
        variance_loading_1=math.sqrt(days) * math.sqrt(residual),
        sigma2=sigma2,
    )


def write_json(path, document):
    path.write_text(json.dumps(document, sort_keys=True, indent=2) + "\n")


def transitions():
    result = []
    for scheme in range(3):
        for variance in [0.04, 0.0, -0.04]:
            config = dict(BASE, discretization=scheme)
            proc = process(config)
            state = ql.Array([config["spot"], variance])
            matrix = proc.diffusion(0, state)
            for sign in [-1, 1]:
                dt, dw, dx = 1 / 252, [sign * 0.7, -0.2], [0.01, -0.03]
                result.append(dict(
                    name=f"scheme_{scheme}_variance_{variance}_shock_{sign}",
                    input=parameters(config), state=list(state), dt=dt, dw=dw, dx=dx,
                    initial=list(proc.initialValues()), drift=list(proc.drift(0, state)),
                    diffusion=[[matrix[i][j] for j in range(2)] for i in range(2)],
                    evolve=list(proc.evolve(0, state, dt, ql.Array(dw))),
                    apply=[state[0] * math.exp(dx[0]), state[1] + dx[1]],
                ))
    for name, changes in [
        ("positive_lambda_negative_gamma", dict(lambda_parameter=0.3, gamma=-0.02)),
        ("zero_lambda", dict(lambda_parameter=0.0)),
        ("nonstationary", dict(beta=1.0)),
        ("zero_initial_variance", dict(daily_variance=0.0)),
        ("zero_omega", dict(omega=0.0)),
        ("days_365", dict(days_per_year=365.0)),
    ]:
        config = dict(BASE, **changes)
        proc, state, dw, dt = process(config), ql.Array([100, 0.04]), [0.7, -0.2], 1 / 252
        matrix = proc.diffusion(0, state)
        coeff = coefficients(config)
        assert math.isclose(matrix[1][0], state[1] * coeff["variance_loading_0"], abs_tol=2e-15)
        assert math.isclose(matrix[1][1], state[1] * coeff["variance_loading_1"], abs_tol=2e-15)
        result.append(dict(
            name=name, input=parameters(config), state=list(state), dt=dt, dw=dw,
            initial=list(proc.initialValues()), drift=list(proc.drift(0, state)),
            diffusion=[[matrix[i][j] for j in range(2)] for i in range(2)],
            evolve=list(proc.evolve(0, state, dt, ql.Array(dw))), coefficients=coeff,
        ))
    for scheme in range(3):
        config = dict(BASE, discretization=scheme)
        proc, state = process(config), ql.Array([100, -0.04])
        matrix = proc.diffusion(0, state)
        result.append(dict(
            name=f"zero_dt_negative_scheme_{scheme}", input=parameters(config),
            state=list(state), dt=0.0, dw=[0.7, -0.2],
            initial=list(proc.initialValues()), drift=list(proc.drift(0, state)),
            diffusion=[[matrix[i][j] for j in range(2)] for i in range(2)],
            evolve=list(proc.evolve(0, state, 0.0, ql.Array([0.7, -0.2]))),
        ))
    return result


def paths():
    result, records = [], []
    for name, changes in [
        ("base", {}),
        ("negative_partial", dict(alpha=0.3, beta=0.4, gamma=2.0, discretization=0)),
        ("negative_full", dict(alpha=0.3, beta=0.4, gamma=2.0, discretization=1)),
        ("negative_reflection", dict(alpha=0.3, beta=0.4, gamma=2.0, discretization=2)),
        ("negative_gamma", dict(gamma=-0.02, lambda_parameter=0.3)),
        ("zero_horizon", dict(horizon=0.0)),
    ]:
        config = dict(BASE, horizon=4 / 252, steps=4, paths=3, seed=42, terminal_only=False)
        config.update(changes)
        proc, rng = process(config), MT19937(config["seed"])
        full, terminal = [], []
        dt = config["horizon"] / config["steps"]
        for path in range(config["paths"]):
            state = proc.initialValues()
            full.extend(state)
            for step in range(config["steps"]):
                words = [rng.word(), rng.word()]
                dw = [normal((word + 0.5) / 2**32) for word in words]
                state = proc.evolve(step * dt, state, dt, ql.Array(dw))
                full.extend(state)
                if name == "base":
                    records.append(dict(path=path, step=step + 1, words=words, dw=dw, state=list(state)))
            terminal.extend(state)
        assert all(math.isfinite(x) for x in full)
        if name.startswith("negative_") and name != "negative_gamma":
            assert any(x < 0 for x in full[1::2])
        result.append(dict(name=name, input=parameters(config), expected_full=full, expected_terminal=terminal))
    return result, records


def moment_reference():
    config = dict(BASE)
    coeff, variance, dt = coefficients(config), 0.04, 1 / 252
    variance_drift = config["days_per_year"]**2 * config["omega"] + config["days_per_year"] * (coeff["persistence"] - 1) * variance
    variance_sd = math.sqrt(dt) * variance * math.hypot(coeff["variance_loading_0"], coeff["variance_loading_1"])
    drift = config["risk_free_rate"] - config["dividend_yield"]
    return dict(
        input=parameters(config), state=[100.0, variance], dt=dt,
        expected_spot_mean=100 * math.exp(drift * dt),
        expected_spot_second_moment=100**2 * math.exp((2 * drift + variance) * dt),
        expected_variance_mean=variance + variance_drift * dt,
        expected_variance_variance=variance_sd**2,
        expected_log_spot_variance_covariance=variance**1.5 * coeff["variance_loading_0"] * dt,
    )


def moment_proof(reference):
    count = 40000
    rng, proc = MT19937(5489), process(BASE)
    spots, variances, log_spots = [], [], []
    for _ in range(count):
        dw = [normal((rng.word() + 0.5) / 2**32) for _ in range(2)]
        state = proc.evolve(0, ql.Array(reference["state"]), reference["dt"], ql.Array(dw))
        spots.append(state[0])
        variances.append(state[1])
        log_spots.append(math.log(state[0] / reference["state"][0]))
    second = reference["expected_spot_second_moment"]
    fourth = 100**4 * math.exp((4 * 0.02 + 6 * 0.04) * reference["dt"])
    variance_variance = reference["expected_variance_variance"]
    covariance = reference["expected_log_spot_variance_covariance"]
    observations = [
        statistics.fmean(spots), statistics.fmean(x * x for x in spots),
        statistics.fmean(variances), statistics.variance(variances),
        statistics.covariance(log_spots, variances),
    ]
    expected = [
        reference["expected_spot_mean"], second, reference["expected_variance_mean"],
        variance_variance, covariance,
    ]
    errors = [
        math.sqrt((second - expected[0]**2) / count),
        math.sqrt((fourth - second**2) / count),
        math.sqrt(variance_variance / count),
        variance_variance * math.sqrt(2 / (count - 1)),
        math.sqrt((0.04 * reference["dt"] * variance_variance + covariance**2) / (count - 1)),
    ]
    deviations = [(value - target) / error for value, target, error in zip(observations, expected, errors)]
    assert max(abs(x) for x in deviations) < 6.0
    return dict(
        count=count, seed=5489, observed=observations, expected=expected,
        standard_errors=errors, standard_deviations=deviations,
        ordering="spotmean,spotsecond,variancemean,variancevariance,logspot_variancecovariance",
        generator="one-step native QuantLib transitions with independent MT/Acklam draws",
    )


def build(output):
    assert ql.__version__ == "1.43", ql.__version__
    rng = MT19937(5489)
    assert [rng.word() for _ in range(10)] == [
        3499211612, 581869302, 3890346734, 3586334585, 545404204,
        4161255391, 3922919429, 949333985, 2715962298, 1323567403,
    ]
    path_cases, draws = paths()
    provenance = dict(
        quantlib_version=ql.__version__, quantlib_commit=COMMIT,
        reference="ql/processes/gjrgarchprocess.cpp", generator="QuantLib-Python executable transitions",
        paths="independent MT19937/Acklam normals fed into QuantLib evolve",
        apply="independent Python spot*exp(dx0), variance+dx1; SWIG omits apply",
        layout="path,time,state(spot,annual_variance)", uniform="(word+0.5)/4294967296",
        draw_order="path,step,factor0,factor1", relative_tolerance=2e-13, absolute_tolerance=1e-12,
        header_sigma2_typo="executable 2+4*lambda^2, not header lambda^4",
    )
    transition_cases = transitions()
    for scheme in range(3):
        selected = [case for case in transition_cases if case["name"].startswith(f"scheme_{scheme}_") or case["name"] == f"zero_dt_negative_scheme_{scheme}"]
        write_json(output / f"gjrgarch-transitions-{scheme}.json", dict(provenance=provenance, cases=selected))
    selected = [case for case in transition_cases if not case["name"].startswith(("scheme_", "zero_dt_"))]
    write_json(output / "gjrgarch-transitions-edge.json", dict(provenance=provenance, cases=selected))
    write_json(output / "gjrgarch-paths.json", dict(provenance=provenance, cases=path_cases))
    write_json(output / "gjrgarch-draws.json", dict(seed=42, draws=draws))
    write_json(output / "gjrgarch-moments.json", dict(provenance=provenance, reference=moment_reference(), proof=moment_proof(moment_reference())))
    print(json.dumps(dict(transitions=27, paths=len(path_cases), draws=len(draws)), sort_keys=True))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=Path(__file__).resolve().parent)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    build(args.output)
