"""Independent arithmetic, MT stream and Gaussian-quantile replay of local-vol MC."""

import math
import random
from statistics import NormalDist


def stream(seed):
    words = [seed & 0xFFFFFFFF]
    for index in range(1, 624):
        previous = words[-1]
        words.append((1812433253 * (previous ^ (previous >> 30)) + index) & 0xFFFFFFFF)
    generator = random.Random()
    generator.setstate((3, (*words, 624), None))
    normal = NormalDist()
    while True:
        uniform = (generator.getrandbits(32) + 0.5) / 4294967296
        yield normal.inv_cdf(uniform)


def value(case, samples):
    steps = case["steps"]
    dt = case["maturity_days"] / 365 / steps
    end = dt * steps
    intervals = int(end / dt)
    spacing = end / intervals
    gaussian = stream(case["seed"])
    payoffs = []
    for _ in range(samples):
        path = [case["spot"]]
        for _ in range(steps):
            state = path[-1]
            sigma = 0.15 + 0.1 * state / (state + 100)
            drift = (
                case["risk_free_rate"] - case["dividend_yield"] - 0.5 * sigma * sigma
            )
            path.append(
                state * math.exp(drift * dt + sigma * math.sqrt(dt) * next(gaussian))
            )

        def integrand(t, values=path):
            state = values[int(t / dt)]
            sigma = 0.15 + 0.1 * state / (state + 100)
            return sigma * sigma

        total = 0.5 * (integrand(0) + integrand(end))
        t = spacing
        for _ in range(1, intervals):
            total += integrand(t)
            t += spacing
        payoffs.append(total * spacing / end)
    mean = math.fsum(payoffs) / samples
    error = math.sqrt(
        math.fsum((x - mean) ** 2 for x in payoffs) / (samples - 1) / samples
    )
    multiplier = (
        (1 if case["position"] == "long" else -1)
        * math.exp(-case["risk_free_rate"] * case["maturity_days"] / 365)
        * case["notional"]
    )
    return {
        "variance": mean,
        "variance_error": error,
        "npv": multiplier * (mean - case["variance_strike"]),
        "error_estimate": multiplier * error,
    }
