"""Independent QuantLib 1.43 settlement and rate-helper oracles.

Run: uv run --with QuantLib==1.43 rates_completion_oracle.py

The Go tests consume both Simple and Compound OIS cases.
The swaption fixture follows test-suite/swaption.cpp settlement conventions;
OIS helpers use explicit simple/compound averaging and a single log-linear node.
"""

import json

import QuantLib as q

assert q.__version__ == "1.43"
today = q.Date(7, 7, 2026)
q.Settings.instance().evaluationDate = today
cal = q.TARGET()
dc = q.Actual365Fixed()
curve = q.YieldTermStructureHandle(q.FlatForward(today, 0.04, dc))
index = q.Euribor6M(curve)
exercise = cal.advance(today, 1, q.Years)
start = cal.advance(exercise, 2, q.Days)
end = cal.advance(start, 3, q.Years)
fixed = q.Schedule(start, end, q.Period(q.Annual), cal, q.ModifiedFollowing,
                   q.ModifiedFollowing, q.DateGeneration.Backward, False)
floating = q.Schedule(start, end, q.Period(q.Semiannual), cal, q.ModifiedFollowing,
                      q.ModifiedFollowing, q.DateGeneration.Backward, False)
result = {"quantlib": q.__version__, "swaptions": [], "ois": []}
for kind, label in [(q.VanillaSwap.Payer, "payer"), (q.VanillaSwap.Receiver, "receiver")]:
    swap = q.VanillaSwap(kind, 1, fixed, 0.04, dc, floating, index, 0, q.Actual360())
    swap.setPricingEngine(q.DiscountingSwapEngine(curve))
    result[label] = {"npv": swap.NPV(), "fair_rate": swap.fairRate()}
    for st, method, name in [
        (q.Settlement.Physical, q.Settlement.PhysicalCleared, "physical_cleared"),
        (q.Settlement.Cash, q.Settlement.CollateralizedCashPrice, "collateralized_cash"),
        (q.Settlement.Cash, q.Settlement.ParYieldCurve, "par_yield"),
    ]:
        for model, model_name in [(q.BlackSwaptionEngine.SwapRate, "swap_rate"),
                                  (q.BlackSwaptionEngine.DiscountCurve, "discount_curve")]:
            option = q.Swaption(swap, q.EuropeanExercise(exercise), st, method)
            option.setPricingEngine(q.BlackSwaptionEngine(
                curve, q.QuoteHandle(q.SimpleQuote(0.20)), dc, 0, model))
            result["swaptions"].append({"side": label, "settlement": name,
                                       "annuity": model_name, "npv": option.NPV()})
for averaging, label in [(q.RateAveraging.Simple, "simple"),
                          (q.RateAveraging.Compound, "compound")]:
    helper = q.OISRateHelper(2, q.Period(1, q.Years), 0.05, q.Estr(),
                            paymentLag=2, averagingMethod=averaging)
    bootstrapped = q.PiecewiseLogLinearDiscount(today, [helper], q.Actual360())
    end_discount = bootstrapped.discount(helper.maturityDate())
    result["ois"].append({"averaging": label,
                          "maturity": helper.maturityDate().ISO(),
                          "pillar": helper.pillarDate().ISO(),
                          "discount": end_discount, "quote": helper.impliedQuote()})
print(json.dumps(result, indent=2, sort_keys=True))
