"""Independent C++ oracle: uv run --with QuantLib==1.43 this_file.py."""
import json

import QuantLib as ql

ql.Settings.instance().evaluationDate = ql.Date(15, 2, 2002)
curve = ql.YieldTermStructureHandle(
    ql.FlatForward(ql.Date(19, 2, 2002), 0.04875825, ql.Actual365Fixed())
)
index = ql.Euribor6M(curve)
results = {"quantlib_version": ql.__version__, "cases": {}}
for name, kind in (
    ("price", ql.BlackCalibrationHelper.PriceError),
    ("implied_vol", ql.BlackCalibrationHelper.ImpliedVolError),
):
    model = ql.HullWhite(curve, 0.1, 0.01)
    engine = ql.JamshidianSwaptionEngine(model)
    helpers = []
    for i, vol in enumerate((0.1148, 0.1108, 0.1070, 0.1021, 0.1000)):
        helper = ql.SwaptionHelper(
            ql.Period(i + 1, ql.Years), ql.Period(5 - i, ql.Years),
            ql.QuoteHandle(ql.SimpleQuote(vol)), index, ql.Period(1, ql.Years),
            ql.Thirty360(ql.Thirty360.BondBasis), ql.Actual360(), curve, kind,
        )
        helper.setPricingEngine(engine)
        helpers.append(helper)
    model.calibrate(
        helpers, ql.LevenbergMarquardt(1e-8, 1e-8, 1e-8),
        ql.EndCriteria(10000, 100, 1e-6, 1e-8, 1e-8),
    )
    results["cases"][name] = {
        "parameters": list(model.params()),
        "errors": [helper.calibrationError() for helper in helpers],
    }
print(json.dumps(results, indent=2))
