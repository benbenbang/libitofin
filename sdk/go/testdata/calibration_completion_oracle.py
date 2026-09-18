"""QuantLib 1.43 oracle: uv run --with QuantLib==1.43 this_file.py.

Heston uses a perturbed synthetic smile to distinguish signed error metrics.
Hull-White follows test-suite/shortratemodels.cpp:testCachedHullWhite.
"""

import json

import QuantLib as ql

results = {"quantlib_version": ql.__version__, "heston": {}, "hull_white": {}}
ql.Settings.instance().evaluationDate = today = ql.Date(15, 1, 2026)
dc = ql.Actual365Fixed()
rate = ql.YieldTermStructureHandle(ql.FlatForward(today, 0.03, dc))
dividend = ql.YieldTermStructureHandle(ql.FlatForward(today, 0.01, dc))
spot = ql.QuoteHandle(ql.SimpleQuote(100))
reference = ql.HestonModel(
    ql.HestonProcess(rate, dividend, spot, 0.04, 1.2, 0.05, 0.3, -0.5)
)
engine = ql.AnalyticHestonEngine(reference, 96)
quotes = []
for months in (6, 12, 24):
    for strike in (80, 100, 120):
        helper = ql.HestonModelHelper(
            ql.Period(months, ql.Months),
            ql.NullCalendar(),
            100,
            strike,
            ql.QuoteHandle(ql.SimpleQuote(0.2)),
            rate,
            dividend,
        )
        helper.setPricingEngine(engine)
        quotes.append(
            [
                months,
                strike,
                helper.impliedVolatility(helper.modelValue(), 1e-12, 1000, 0.001, 5.0),
            ]
        )
for quote, perturbation in zip(
    quotes, (0.0003, -0.0001, 0.0002, -0.0002, 0.0001, -0.0003, 0.0001, -0.0002, 0.0003)
):
    quote[2] += perturbation
results["quotes"] = quotes
for name, kind in (
    ("price", ql.BlackCalibrationHelper.PriceError),
    ("implied_vol", ql.BlackCalibrationHelper.ImpliedVolError),
):
    model = ql.HestonModel(
        ql.HestonProcess(rate, dividend, spot, 0.035, 1.0, 0.045, 0.25, -0.4)
    )
    engine = ql.AnalyticHestonEngine(model, 96)
    helpers = []
    for months, strike, volatility in quotes:
        helper = ql.HestonModelHelper(
            ql.Period(months, ql.Months),
            ql.NullCalendar(),
            100,
            strike,
            ql.QuoteHandle(ql.SimpleQuote(volatility)),
            rate,
            dividend,
            kind,
        )
        helper.setPricingEngine(engine)
        helpers.append(helper)
    model.calibrate(
        helpers,
        ql.LevenbergMarquardt(1e-8, 1e-8, 1e-8),
        ql.EndCriteria(1000, 100, 1e-8, 1e-8, 1e-8),
    )
    results["heston"][name] = {
        "parameters": [
            model.v0(),
            model.kappa(),
            model.theta(),
            model.sigma(),
            model.rho(),
        ],
        "errors": [helper.calibrationError() for helper in helpers],
    }
ql.Settings.instance().evaluationDate = ql.Date(15, 2, 2002)
curve = ql.YieldTermStructureHandle(
    ql.FlatForward(ql.Date(19, 2, 2002), 0.04875825, ql.Actual365Fixed())
)
index = ql.Euribor6M(curve)
for name, kind in (
    ("price", ql.BlackCalibrationHelper.PriceError),
    ("implied_vol", ql.BlackCalibrationHelper.ImpliedVolError),
):
    for fixed in (False, True):
        model = ql.HullWhite(curve, 0.05 if fixed else 0.1, 0.01)
        engine = ql.JamshidianSwaptionEngine(model)
        helpers = []
        for i, vol in enumerate((0.1148, 0.1108, 0.1070, 0.1021, 0.1000)):
            helper = ql.SwaptionHelper(
                ql.Period(i + 1, ql.Years),
                ql.Period(5 - i, ql.Years),
                ql.QuoteHandle(ql.SimpleQuote(vol)),
                index,
                ql.Period(1, ql.Years),
                ql.Thirty360(ql.Thirty360.BondBasis),
                ql.Actual360(),
                curve,
                kind,
            )
            helper.setPricingEngine(engine)
            helpers.append(helper)
        model.calibrate(
            helpers,
            ql.LevenbergMarquardt(1e-8, 1e-8, 1e-8),
            ql.EndCriteria(10000, 100, 1e-6, 1e-8, 1e-8),
            ql.NoConstraint(),
            [],
            [fixed, False],
        )
        results["hull_white"][name + ("_fixed" if fixed else "_free")] = {
            "parameters": list(model.params()),
            "errors": [helper.calibrationError() for helper in helpers],
        }
print(json.dumps(results, indent=2, sort_keys=True))
