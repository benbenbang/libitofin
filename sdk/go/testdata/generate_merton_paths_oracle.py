import argparse
import json
import math
from pathlib import Path
import statistics
import sys

MASK = (1 << 32) - 1
OFFSETS = (0x9E3779B9, 0xBB67AE85)
BASE = dict(
    spot=100.0,
    drift=0.05,
    volatility=0.2,
    jump_intensity=1.0,
    log_mean_jump=-0.1,
    log_jump_volatility=0.3,
    horizon=1.0,
    steps=4,
    paths=3,
    seed=42,
    terminal_only=False,
)
A = (
    -39.69683028665376,
    220.9460984245205,
    -275.9285104469687,
    138.3577518672690,
    -30.66479806614716,
    2.506628277459239,
)
B = (
    -54.47609879822406,
    161.5858368580409,
    -155.6989798598866,
    66.80131188771972,
    -13.28068155288572,
    1.0,
)
C = (
    -0.007784894002430293,
    -0.3223964580411365,
    -2.400758277161838,
    -2.549732539343734,
    4.374664141464968,
    2.938163982698783,
)
D = (
    0.007784695709041462,
    0.3224671290700398,
    2.445134137142996,
    3.754408661907416,
    1.0,
)


class MT19937:
    def __init__(self, seed):
        self.state = [seed]
        for i in range(1, 624):
            prev = self.state[-1]
            self.state.append((1812433253 * (prev ^ (prev >> 30)) + i) & MASK)
        self.index = 624

    def word(self):
        if self.index == 624:
            for i in range(624):
                bits = (self.state[i] & 0x80000000) | (
                    self.state[(i + 1) % 624] & 0x7FFFFFFF
                )
                self.state[i] = (
                    self.state[(i + 397) % 624]
                    ^ (bits >> 1)
                    ^ (0x9908B0DF if bits & 1 else 0)
                )
            self.index = 0
        value = self.state[self.index]
        self.index += 1
        value ^= value >> 11
        value ^= (value << 7) & 0x9D2C5680
        value ^= (value << 15) & 0xEFC60000
        return (value ^ (value >> 18)) & MASK


def horner(coefficients, x):
    result = coefficients[0]
    for coefficient in coefficients[1:]:
        result = result * x + coefficient
    return result


def normal(p):
    if 0.02425 <= p <= 1.0 - 0.02425:
        z = p - 0.5
        return horner(A, z * z) * z / horner(B, z * z)
    z = math.sqrt(-2.0 * math.log(p if p < 0.02425 else 1.0 - p))
    result = horner(C, z) / horner(D, z)
    return result if p < 0.02425 else -result


def poisson(mean, p):
    if mean == 0.0:
        return 0, 0.0, 1.0
    mass = math.exp(-mean)
    if mass < sys.float_info.min:
        raise ValueError("Poisson recurrence seed is subnormal")
    cumulative = 0.0
    count = 0
    while True:
        previous = cumulative
        cumulative += mass
        if p <= cumulative:
            return count, previous, cumulative
        if cumulative == previous:
            raise ValueError("Poisson CDF plateau")
        count += 1
        mass *= mean / count


def seeds(seed):
    return dict(
        diffusion=seed,
        poisson=1 + (seed - 1 + OFFSETS[0]) % MASK,
        jump=1 + (seed - 1 + OFFSETS[1]) % MASK,
    )


def simulate(config, record=False):
    streams = [MT19937(seed) for seed in seeds(config["seed"]).values()]
    dt = config["horizon"] / config["steps"]
    sigma = config["volatility"]
    intensity = config["jump_intensity"]
    jump_mu = config["log_mean_jump"]
    jump_sigma = config["log_jump_volatility"]
    kappa = math.expm1(jump_mu + 0.5 * jump_sigma * jump_sigma)
    center = (config["drift"] - intensity * kappa - 0.5 * sigma * sigma) * dt
    scale = sigma * math.sqrt(dt)
    full, terminal, records, path_counts = [], [], [], []
    for path in range(config["paths"]):
        value = config["spot"]
        full.append(value)
        total_count = 0
        for step in range(config["steps"]):
            words = [stream.word() for stream in streams]
            uniforms = [(word + 0.5) / (1 << 32) for word in words]
            zd = normal(uniforms[0])
            count, low, high = poisson(intensity * dt, uniforms[1])
            zj = normal(uniforms[2])
            total_count += count
            if config["horizon"] == 0.0:
                value = config["spot"]
            elif intensity == 0.0 and sigma == 0.0:
                time = (
                    config["horizon"]
                    if step + 1 == config["steps"]
                    else (step + 1) * dt
                )
                value = config["spot"] * math.exp(config["drift"] * time)
            else:
                log_increment = (
                    center
                    + scale * zd
                    + count * jump_mu
                    + math.sqrt(count) * jump_sigma * zj
                )
                value *= math.exp(log_increment)
                if not math.isfinite(value) or value <= 0.0:
                    raise ValueError(
                        f"Invalid spot at path={path},step={step},value={value}"
                    )
            full.append(value)
            if record:
                records.append(
                    dict(
                        path=path,
                        step=step + 1,
                        words=words,
                        uniforms=uniforms,
                        diffusion_normal=zd,
                        jump_count=count,
                        jump_normal=zj,
                        poisson_cdf_below=low,
                        poisson_cdf_at=high,
                        spot=value,
                    )
                )
        terminal.append(value)
        path_counts.append(total_count)
    return full, terminal, records, path_counts


def moment(config, power):
    delta = config["log_jump_volatility"]
    mu = config["log_mean_jump"]
    kappa = math.expm1(mu + 0.5 * delta * delta)
    exponent = (
        power * config["drift"]
        + 0.5 * power * (power - 1) * config["volatility"] ** 2
        + config["jump_intensity"]
        * (math.expm1(power * mu + 0.5 * power * power * delta * delta) - power * kappa)
    ) * config["horizon"]
    return config["spot"] ** power * math.exp(exponent)


def build(output):
    mt = MT19937(5489)
    assert [mt.word() for _ in range(10)] == [
        3499211612,
        581869302,
        3890346734,
        3586334585,
        545404204,
        4161255391,
        3922919429,
        949333985,
        2715962298,
        1323567403,
    ]
    variants = [
        ("base", {}),
        ("multiple_jumps", dict(jump_intensity=8.0)),
        ("positive_log_mean", dict(jump_intensity=3.0, log_mean_jump=0.08)),
        ("deterministic_jump_size", dict(jump_intensity=8.0, log_jump_volatility=0.0)),
        ("zero_intensity", dict(jump_intensity=0.0)),
        ("zero_diffusion", dict(volatility=0.0, jump_intensity=8.0)),
        ("zero_horizon", dict(horizon=0.0)),
        (
            "poisson_mean_60",
            dict(jump_intensity=240.0, log_mean_jump=0.0, log_jump_volatility=0.01),
        ),
        (
            "poisson_mean_700",
            dict(jump_intensity=2800.0, log_mean_jump=0.0, log_jump_volatility=0.005),
        ),
        ("different_seed", dict(seed=20261002)),
        ("maximum_seed", dict(seed=MASK)),
        (
            "zero_intensity_and_diffusion",
            dict(jump_intensity=0.0, volatility=0.0, horizon=0.7, steps=3),
        ),
    ]
    cases = []
    base_records = None
    for name, changes in variants:
        config = dict(BASE, **changes)
        full, terminal, records, _ = simulate(config, record=name == "base")
        _, repeated, _, _ = simulate(dict(config, terminal_only=True))
        assert [x.hex() for x in terminal] == [x.hex() for x in repeated]
        assert [
            full[(p + 1) * (config["steps"] + 1) - 1] for p in range(config["paths"])
        ] == terminal
        cases.append(
            dict(
                name=name,
                input=config,
                seeds=seeds(config["seed"]),
                expected_full=full,
                expected_terminal=terminal,
            )
        )
        if records:
            base_records = dict(
                input=config, seeds=seeds(config["seed"]), draws=records
            )
    provenance = dict(
        model="Exact constant-parameter Merton transition",
        generator="Independent Python standard-library MT19937/Acklam/Poisson",
        relative_tolerance=2e-14,
        absolute_tolerance=2e-14,
        diffusion_seed="seed",
        poisson_seed_offset=OFFSETS[0],
        jump_seed_offset=OFFSETS[1],
        shifted_seed="1 + ((seed - 1 + offset) % 4294967295)",
        uniform="(word + 0.5) / 4294967296",
        normal="Acklam, no refinement",
        counts="original jump_intensity * dt, upward Poisson CDF recurrence",
    )
    for name, selected in [
        ("merton-paths-oracle.json", cases[:6]),
        ("merton-paths-additional.json", cases[6:]),
    ]:
        (output / name).write_text(
            json.dumps(
                dict(provenance=provenance, cases=selected), indent=2, sort_keys=True
            )
            + "\n"
        )
    (output / "merton-paths-draws.json").write_text(
        json.dumps(base_records, indent=2, sort_keys=True) + "\n"
    )
    statistical = dict(
        BASE,
        paths=40000,
        jump_intensity=2.0,
        log_mean_jump=-0.07,
        log_jump_volatility=0.15,
    )
    _, terminal, _, counts = simulate(statistical)
    mean, second, fourth = [moment(statistical, k) for k in (1, 2, 4)]
    observations = [
        statistics.fmean(terminal),
        statistics.fmean(x * x for x in terminal),
    ]
    errors = [
        math.sqrt((second - mean * mean) / statistical["paths"]),
        math.sqrt((fourth - second * second) / statistical["paths"]),
    ]
    deviations = [
        (value - expected) / error
        for value, expected, error in zip(observations, [mean, second], errors)
    ]
    assert max(abs(x) for x in deviations) < 6.0
    count_mean = statistics.fmean(counts)
    count_variance = statistics.variance(counts)
    expected_count = statistical["jump_intensity"] * statistical["horizon"]
    assert abs(count_mean - expected_count) < 6.0 * math.sqrt(
        expected_count / statistical["paths"]
    )
    assert abs(count_variance - expected_count) < 6.0 * math.sqrt(
        (expected_count + 2 * expected_count * expected_count) / statistical["paths"]
    )
    proof = dict(
        input=statistical,
        analytical_mean=mean,
        analytical_second_moment=second,
        observed_mean=observations[0],
        observed_second_moment=observations[1],
        standard_errors=errors,
        standard_deviations=deviations,
        expected_count_mean_variance=expected_count,
        observed_count_mean=count_mean,
        observed_count_variance=count_variance,
        canonical_mt_vector_verified=True,
        all_full_terminal_exact=True,
    )
    (output / "merton-paths-moment-proof.json").write_text(
        json.dumps(proof, indent=2, sort_keys=True) + "\n"
    )
    print(json.dumps(proof, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=Path(__file__).resolve().parent)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    build(args.output)
