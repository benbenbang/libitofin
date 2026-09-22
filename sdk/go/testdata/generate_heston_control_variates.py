"""Regenerate with QuantLib-Python 1.43, independently of libitofin.

The market is QuantLib's testCosHestonEngine call at strike 120. Its high
volatility of variance provides a well-behaved forced asymptotic case. Each
control variate has its own finite-quadrature result; their numerical prices
need not equal the converged COS reference. Outputs record those differences.
"""

import json
from pathlib import Path

import QuantLib as ql


def generate():
    assert ql.__version__ == "1.43"
    reference = ql.Date(7, 2, 2017)
    ql.Settings.instance().evaluationDate = reference
    dc = ql.Actual365Fixed()
    risk_free = ql.YieldTermStructureHandle(ql.FlatForward(reference, .15, dc))
    dividend = ql.YieldTermStructureHandle(ql.FlatForward(reference, .07, dc))
    process = ql.HestonProcess(risk_free, dividend, ql.QuoteHandle(ql.SimpleQuote(100)), .1, 4, .22, 1.8, -.75)
    model = ql.HestonModel(process)
    option = ql.VanillaOption(ql.PlainVanillaPayoff(ql.Option.Call, 120), ql.EuropeanExercise(ql.Date(7, 2, 2018)))
    names = ["OptimalCV", "AndersenPiterbarg", "AndersenPiterbargOptCV", "AsymptoticChF", "AngledContour", "AngledContourNoCV"]
    prices = []
    for name in names:
        option.setPricingEngine(ql.ExponentialFittingHestonEngine(model, getattr(ql.AnalyticHestonEngine, name)))
        prices.append(option.NPV())
    option.setPricingEngine(ql.ExponentialFittingHestonEngine(model, ql.AnalyticHestonEngine.OptimalCV, 1.0))
    fixture = {
        "source": "QuantLib-Python 1.43 ExponentialFittingHestonEngine, testCosHestonEngine market",
        "reference": "2017-02-07", "expiry": "2018-02-07", "risk_free": .15, "dividend": .07,
        "spot": 100, "v0": .1, "kappa": 4, "theta": .22, "sigma": 1.8, "rho": -.75,
        "strike": 120, "alpha": -.5, "control_variates": names, "prices": prices,
        "fixed_scaling": 1.0, "fixed_scaling_price": option.NPV(),
    }
    Path(__file__).with_name("heston_control_variates.json").write_text(json.dumps(fixture, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    generate()
