"""Regenerate with QuantLib 1.43; mirrors test-suite/bermudanswaption.cpp."""

import json

import QuantLib as q

assert q.__version__ == "1.43"
q.Settings.instance().evaluationDate = q.Date(15, 2, 2002)
cal = q.TARGET()
settlement = cal.advance(q.Settings.instance().evaluationDate, 2, q.Days)
start = cal.advance(settlement, 1, q.Years)
end = cal.advance(start, 5, q.Years)
fixed = q.Schedule(start, end, q.Period(q.Annual), cal, q.Unadjusted,
                   q.Unadjusted, q.DateGeneration.Forward, False)
floating = q.Schedule(start, end, q.Period(q.Semiannual), cal, q.ModifiedFollowing,
                      q.ModifiedFollowing, q.DateGeneration.Forward, False)
rows = []
for at_par in [False, True]:
    if at_par:
        q.IborCoupon.createAtParCoupons()
    else:
        q.IborCoupon.createIndexedCoupons()
    quote = q.SimpleQuote(0.04875825)
    curve = q.YieldTermStructureHandle(q.FlatForward(settlement, q.QuoteHandle(quote), q.Actual365Fixed()))
    index = q.Euribor6M(curve)

    def swap(rate):
        result = q.VanillaSwap(q.VanillaSwap.Payer, 1000, fixed, rate,
                              q.Thirty360(q.Thirty360.BondBasis), floating, index, 0, q.Actual360())
        result.setPricingEngine(q.DiscountingSwapEngine(curve))
        return result

    atm = swap(0).fairRate()
    for multiplier in [0.8, 1.0, 1.2]:
        quote.setValue(0.04875825)
        option = q.Swaption(swap(multiplier * atm), q.BermudanExercise(list(fixed)[:-1]))
        option.setPricingEngine(q.TreeSwaptionEngine(q.HullWhite(curve, 0.048696, 0.0058904), 50))
        initial = option.NPV()
        quote.setValue(0.06)
        bumped = option.NPV()
        rows.append({"at_par": at_par, "multiplier": multiplier, "initial": initial, "bumped": bumped})
print(json.dumps({"quantlib": q.__version__, "rows": rows}, indent=2, sort_keys=True))
