"""Reproduce the tree cap/floor oracle with independent QuantLib 1.43."""

import json

import QuantLib as q

assert q.__version__ == "1.43"
reference = q.Date(15, 1, 2026)
q.Settings.instance().evaluationDate = reference
curve = q.YieldTermStructureHandle(q.FlatForward(reference, 0.03, q.Actual365Fixed()))
model = q.HullWhite(curve, 0.05, 0.01)
schedule = q.Schedule(
    q.Date(15, 1, 2027),
    q.Date(15, 1, 2030),
    q.Period(6, q.Months),
    q.TARGET(),
    q.Unadjusted,
    q.Unadjusted,
    q.DateGeneration.Forward,
    False,
)
leg = q.IborLeg(
    [1e6],
    schedule,
    q.Euribor6M(curve),
    q.Actual360(),
    q.Unadjusted,
    gearings=[1.3],
    spreads=[0.002],
)
results = []
for steps in (30, 100):
    for kind, instrument in (
        ("cap", q.Cap(leg, [0.04])),
        ("floor", q.Floor(leg, [0.025])),
        ("collar", q.Collar(leg, [0.04], [0.025])),
    ):
        instrument.setPricingEngine(q.TreeCapFloorEngine(model, steps))
        results.append({"steps": steps, "type": kind, "npv": instrument.NPV()})
cap = q.Cap(leg, [0.04])
cap.setPricingEngine(q.AnalyticCapFloorEngine(model))
analytic = cap.NPV()
helpers, helper_results = [], []
for length in (2, 3, 4, 5):
    helper = q.CapHelper(
        q.Period(length, q.Years),
        q.QuoteHandle(q.SimpleQuote(0.01)),
        q.Euribor6M(curve),
        q.Annual,
        q.Actual365Fixed(),
        False,
        curve,
        q.BlackCalibrationHelper.RelativePriceError,
        q.Normal,
        0.0,
    )
    helper.setPricingEngine(q.TreeCapFloorEngine(model, 30))
    helper_results.append(
        {"length": length, "market": helper.marketValue(), "model": helper.modelValue()}
    )
    helpers.append(helper)
model.calibrate(
    helpers,
    q.LevenbergMarquardt(1e-8, 1e-8, 1e-8),
    q.EndCriteria(10000, 100, 1e-6, 1e-8, 1e-8),
    q.NoConstraint(),
    [],
    [True, False],
)
print(
    json.dumps(
        {
            "quantlib": q.__version__,
            "results": results,
            "analytic_cap": analytic,
            "normal_helpers": helper_results,
            "fitted_parameters": list(model.params()),
        },
        indent=2,
    )
)
