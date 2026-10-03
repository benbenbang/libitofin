"""Independent QuantLib model oracle configuration and replay helpers."""

import math

import QuantLib as ql

COMMIT = "9863b578af0caa4cecabf697196533e84a8308b6"
DATE = ql.Date(15, 1, 2026)
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
ANALYTIC = [
    [
        15.4315,
        10.5552,
        5.9625,
        2.3282,
        0.5408,
        0.0835,
        15.8969,
        11.2173,
        6.9112,
        3.4788,
        1.3769,
        0.4357,
    ],
    [
        15.4556,
        10.6929,
        6.2381,
        2.6831,
        0.7822,
        0.1738,
        16.0587,
        11.5338,
        7.3170,
        3.9074,
        1.7279,
        0.6568,
    ],
    [
        15.8000,
        11.2734,
        7.0376,
        3.6767,
        1.5871,
        0.5934,
        16.9286,
        12.3170,
        8.0405,
        4.6348,
        2.3429,
        1.0590,
    ],
]
MC = [
    [
        15.4332,
        10.5453,
        5.9351,
        2.3521,
        0.5597,
        0.0776,
        15.8910,
        11.1772,
        6.8827,
        3.5096,
        1.4196,
        0.4502,
    ],
    [
        15.4580,
        10.6433,
        6.2019,
        2.7513,
        0.8374,
        0.1706,
        15.9884,
        11.4139,
        7.3103,
        4.0497,
        1.8862,
        0.7322,
    ],
    [
        15.6619,
        11.1263,
        7.0968,
        3.9152,
        1.8133,
        0.7010,
        16.5195,
        12.3181,
        8.6085,
        5.5700,
        3.3103,
        1.8053,
    ],
]


def market(config, date=DATE, dc=None):
    dc = dc or ql.Actual365Fixed()
    spot, rate, div = [
        ql.SimpleQuote(config[key])
        for key in ["spot", "risk_free_rate", "dividend_yield"]
    ]
    rf = ql.YieldTermStructureHandle(ql.FlatForward(date, ql.QuoteHandle(rate), dc))
    dy = ql.YieldTermStructureHandle(ql.FlatForward(date, ql.QuoteHandle(div), dc))
    proc = ql.GJRGARCHProcess(
        rf,
        dy,
        ql.QuoteHandle(spot),
        config["daily_variance"],
        config["omega"],
        config["alpha"],
        config["beta"],
        config["gamma"],
        config["lambda_parameter"],
        config["days_per_year"],
        config["discretization"],
    )
    return proc, (spot, rate, div)


def option(engine, strike, days, kind=ql.Option.Call, date=DATE):
    result = ql.VanillaOption(
        ql.PlainVanillaPayoff(kind, strike), ql.EuropeanExercise(date + days)
    )
    result.setPricingEngine(engine)
    return result


def stationary(lam):
    cdf = math.erfc(-lam / math.sqrt(2)) / 2
    m1 = 0.93 + (0.024 + 0.059 * cdf) * (1 + lam * lam)
    m1 += 0.059 * lam * math.exp(-lam * lam / 2) / math.sqrt(2 * math.pi)
    return 0.000002 / (1 - m1)


def replay_count(
    proc, strike, days, expected_price, expected_error, tolerance, settings
):
    count = 1023
    while True:
        engine = ql.MCPREuropeanGJRGARCHEngine(proc, requiredSamples=count, **settings)
        fixed = option(engine, strike, days)
        value, error = fixed.NPV(), fixed.errorEstimate()
        if value == expected_price and error == expected_error:
            return count
        count += int(max(count * error * error / tolerance**2 * 0.8 - count, 1023))
        if count > 1000000:
            raise RuntimeError(
                "native tolerance count could not be independently replayed"
            )
