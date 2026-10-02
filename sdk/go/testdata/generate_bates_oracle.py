"""Regenerate independent Bates fixtures with QuantLib-Python 1.43 only."""

import csv
import json
import math
from pathlib import Path

import QuantLib as ql

COMMIT = "9863b578af0caa4cecabf697196533e84a8308b6"
BASE = [0.05, 1.5, 0.35, -0.6, 0.04, -0.12, 0.18, 0.7]
REFERENCE = [15, 1, 2026]


def market(reference, risk_free, dividend, spot, day_counter):
    date = ql.Date(*reference)
    ql.Settings.instance().evaluationDate = date
    dc = (
        ql.Actual365Fixed()
        if day_counter == "Actual365Fixed"
        else ql.ActualActual(ql.ActualActual.ISDA)
    )
    rf = ql.YieldTermStructureHandle(ql.FlatForward(date, risk_free, dc))
    div = ql.YieldTermStructureHandle(ql.FlatForward(date, dividend, dc))
    return date, dc, rf, div, ql.QuoteHandle(ql.SimpleQuote(spot))


def bates_model(mkt, params):
    theta, kappa, sigma, rho, v0, nu, delta, intensity = params
    return ql.BatesModel(
        ql.BatesProcess(*mkt[2:], v0, kappa, theta, sigma, rho, intensity, nu, delta)
    )


def heston_model(mkt, params):
    theta, kappa, sigma, rho, v0 = params[:5]
    return ql.HestonModel(ql.HestonProcess(*mkt[2:], v0, kappa, theta, sigma, rho))


def heston_engine(model, order):
    integration = ql.AnalyticHestonEngine_Integration.gaussLaguerre(order)
    return ql.AnalyticHestonEngine(model, ql.AnalyticHestonEngine.Gatheral, integration)


def price(mkt, expiry, strike, kind, params, order, heston=False):
    payoff = ql.PlainVanillaPayoff(
        ql.Option.Call if kind == "call" else ql.Option.Put, strike
    )
    option = ql.VanillaOption(payoff, ql.EuropeanExercise(ql.Date(*expiry)))
    model = heston_model(mkt, params) if heston else bates_model(mkt, params)
    engine = heston_engine(model, order) if heston else ql.BatesEngine(model, order)
    option.setPricingEngine(engine)
    return option.NPV()


def deterministic_jump_price(mkt, expiry, strike, kind, params, order):
    nu, intensity = params[5], params[7]
    tau = mkt[1].yearFraction(mkt[0], ql.Date(*expiry))
    mean = intensity * tau
    compensation = -mean * math.expm1(nu)
    weight = math.exp(-mean)
    total_weight = 0.0
    terms = []
    for count in range(128):
        spot = mkt[4].value() * math.exp(compensation + count * nu)
        shifted = (*mkt[:4], ql.QuoteHandle(ql.SimpleQuote(spot)))
        terms.append(
            weight * price(shifted, expiry, strike, kind, params, order, heston=True)
        )
        total_weight += weight
        weight *= mean / (count + 1)
    assert abs(1.0 - total_weight) < 1e-14
    return math.fsum(terms)


def calibration(mkt):
    target = bates_model(mkt, BASE)
    engine = ql.BatesEngine(target, 144)
    helpers = []
    data = []
    for months in [3, 6, 12, 24]:
        for strike in [80, 90, 100, 110, 120]:
            quote = ql.SimpleQuote(0.2)
            helper = ql.HestonModelHelper(
                ql.Period(months, ql.Months),
                ql.NullCalendar(),
                100,
                strike,
                ql.QuoteHandle(quote),
                *mkt[2:4],
                ql.BlackCalibrationHelper.RelativePriceError,
            )
            helper.setPricingEngine(engine)
            value = helper.modelValue()
            vol = helper.impliedVolatility(value, 1e-12, 1000, 0.001, 5)
            quote.setValue(vol)
            helpers.append(helper)
            data.append([months, strike, vol, value])
    fits = []
    for name, start, fixed, tolerance, iterations, stationary in [
        ("intensity", [*BASE[:7], 1.2], [True] * 7 + [False], 1e-8, 400, 40),
        (
            "jump_three",
            [*BASE[:5], -0.06, 0.25, 0.3],
            [True] * 5 + [False] * 3,
            1e-12,
            800,
            80,
        ),
    ]:
        model = bates_model(mkt, start)
        fit_engine = ql.BatesEngine(model, 144)
        for helper in helpers:
            helper.setPricingEngine(fit_engine)
        method = ql.LevenbergMarquardt(tolerance, tolerance, tolerance, False)
        end = ql.EndCriteria(iterations, stationary, tolerance, tolerance, tolerance)
        model.calibrate(helpers, method, end, ql.NoConstraint(), [], fixed)
        errors = [helper.calibrationError() for helper in helpers]
        assert max(errors) < 1e-8
        assert max(abs(a - b) for a, b in zip(model.params(), BASE)) < 1e-6
        fits.append(
            dict(
                name=name,
                start=start,
                fixed=fixed,
                optimizer_tolerance=tolerance,
                max_iterations=iterations,
                stationary_iterations=stationary,
                fitted=list(model.params()),
                max_relative_error=max(errors),
                squared_relative_error=math.fsum(error * error for error in errors),
            )
        )
    return dict(
        reference=REFERENCE,
        day_counter="Actual365Fixed",
        spot=100,
        risk_free=0.03,
        dividend=0.01,
        target=BASE,
        integration_order=144,
        helper_error="RelativePriceError",
        calendar="NullCalendar",
        helper_columns=["months", "strike", "volatility", "target_price"],
        helpers=data,
        fits=fits,
        parameter_tolerance=1e-6,
        max_relative_error_tolerance=1e-8,
    )


def generate():
    assert ql.__version__ == "1.43"
    cases = []

    def add(
        name,
        reference,
        expiry,
        dc,
        rf,
        div,
        spot,
        strike,
        kind,
        params,
        order,
        mode="bates",
    ):
        mkt = market(reference, rf, div, spot, dc)
        if mode == "deterministic_jump_mixture":
            value = deterministic_jump_price(mkt, expiry, strike, kind, params, order)
        else:
            value = price(
                mkt, expiry, strike, kind, params, order, heston=mode == "heston_limit"
            )
        assert math.isfinite(value) and value > 0
        cases.append(
            [
                name,
                reference,
                expiry,
                dc,
                rf,
                div,
                spot,
                strike,
                kind,
                params,
                order,
                mode,
                value,
            ]
        )

    upstream = [
        ("tHout", 0.04, 1.5, 0.04, 0.3, -0.9, 0.025, 0),
        ("IkonenToivanen", 0.0625, 5, 0.16, 0.9, 0.1, 0.1, 0),
        ("KahlJaeckel", 0.16, 1, 0.16, 2, -0.8, 0, 0),
        ("Equity", 0.07, 2, 0.04, 0.55, -0.8, 0.03, 0.035),
    ]
    for name, v0, kappa, theta, sigma, rho, rf, div in upstream:
        add(
            name,
            [30, 3, 2007],
            [30, 3, 2012],
            "ActualActualISDA",
            rf,
            div,
            100,
            100,
            "put",
            [theta, kappa, sigma, rho, v0, -0.2, 0.1, 2],
            160,
        )
    for days in [90, 365, 730]:
        mkt = market(REFERENCE, 0.03, 0.01, 100, "Actual365Fixed")
        expiry_date = mkt[0] + days
        expiry = [
            expiry_date.dayOfMonth(),
            int(expiry_date.month()),
            expiry_date.year(),
        ]
        for strike in [80, 100, 120]:
            for kind in ["call", "put"]:
                name = f"flat_{days}_{strike}_{kind}"
                add(
                    name,
                    REFERENCE,
                    expiry,
                    "Actual365Fixed",
                    0.03,
                    0.01,
                    100,
                    strike,
                    kind,
                    BASE,
                    144,
                )
                add(
                    f"zero_{days}_{strike}_{kind}",
                    REFERENCE,
                    expiry,
                    "Actual365Fixed",
                    0.03,
                    0.01,
                    100,
                    strike,
                    kind,
                    [*BASE[:7], 0],
                    144,
                    "heston_limit",
                )
    for strike in [70, 100, 140]:
        for kind in ["call", "put"]:
            add(
                f"high_jump_{strike}_{kind}",
                REFERENCE,
                [15, 1, 2028],
                "Actual365Fixed",
                -0.01,
                0.04,
                100,
                strike,
                kind,
                [*BASE[:5], 0.08, 0.45, 4],
                160,
            )
    for strike in [80, 100, 120]:
        for kind in ["call", "put"]:
            add(
                f"deterministic_{strike}_{kind}",
                REFERENCE,
                [15, 1, 2027],
                "Actual365Fixed",
                0.03,
                0.01,
                100,
                strike,
                kind,
                [*BASE[:6], 0, BASE[7]],
                144,
                "deterministic_jump_mixture",
            )
    fixture = dict(
        source="QuantLib-Python 1.43; not certified as the source checkout commit",
        source_checkout=COMMIT,
        parameters=["theta", "kappa", "sigma", "rho", "v0", "nu", "delta", "lambda"],
        price_absolute_tolerance=2e-10,
        columns=[
            "name",
            "reference",
            "expiry",
            "day_counter",
            "risk_free",
            "dividend",
            "spot",
            "strike",
            "kind",
            "parameters",
            "integration_order",
            "oracle",
            "price",
        ],
        cases=cases,
        calibration=calibration(market(REFERENCE, 0.03, 0.01, 100, "Actual365Fixed")),
    )
    output = Path(__file__).with_name("bates-oracle.json")
    output.write_text(json.dumps(fixture, indent=2, sort_keys=True) + "\n")
    columns = [
        "name",
        "reference_day",
        "reference_month",
        "reference_year",
        "expiry_day",
        "expiry_month",
        "expiry_year",
        "day_counter",
        "risk_free",
        "dividend",
        "spot",
        "strike",
        "kind",
        *fixture["parameters"],
        "integration_order",
        "oracle",
        "price",
    ]
    with output.with_name("bates-prices.tsv").open("w", newline="") as stream:
        writer = csv.writer(stream, delimiter="\t", lineterminator="\n")
        writer.writerow(columns)
        for row in cases:
            writer.writerow([row[0], *row[1], *row[2], *row[3:9], *row[9], *row[10:]])
    print(
        f"Generated {len(cases)} independent price cases and {len(fixture['calibration']['helpers'])} calibration helpers"
    )


if __name__ == "__main__":
    generate()
