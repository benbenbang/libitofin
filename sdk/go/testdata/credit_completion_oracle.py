"""Generate credit_completion_oracle.json with the independent QuantLib 1.43 wheel."""

import json

import QuantLib as ql

assert ql.__version__ == "1.43"
today = ql.Date(15, 6, 2026)
end = ql.Date(20, 6, 2029)
ql.Settings.instance().evaluationDate = today
calendar = ql.WeekendsOnly()
dc = ql.Actual365Fixed()
schedule = ql.Schedule(today, end, ql.Period(ql.Quarterly), calendar,
                       ql.Following, ql.Unadjusted, ql.DateGeneration.CDS, False)
discount = ql.FlatForward(today, .03, dc)
hazard = ql.FlatHazardRate(today, ql.QuoteHandle(ql.SimpleQuote(.02)), dc)
engine = ql.MidPointCdsEngine(ql.DefaultProbabilityTermStructureHandle(hazard), .4,
                             ql.YieldTermStructureHandle(discount))
nominal = 1e7
spread = .01


def values(cds):
    """Capture separately signed legs as well as net and fair quotes."""
    return dict(npv=cds.NPV(), coupon=cds.couponLegNPV(), default=cds.defaultLegNPV(),
                fair_spread=cds.fairSpread(), fair_upfront=cds.fairUpfront(),
                rebate=cds.accrualRebate().amount(),
                rebate_date=cds.accrualRebate().date().ISO())


built = ql.MakeCreditDefaultSwap(end, spread, nominal=nominal, tradeDate=today,
                                pricingEngine=engine)
explicit = ql.CreditDefaultSwap(ql.Protection.Buyer, nominal, spread, schedule,
                                ql.Following, ql.Actual360(), True, True, today)
explicit.setPricingEngine(engine)
matching = ql.CreditDefaultSwap(ql.Protection.Buyer, nominal, 0., spread, schedule,
                               ql.Following, ql.Actual360(), True, True, today,
                               calendar.advance(today, 3, ql.Days), None,
                               ql.Actual360(True), True, today)
matching.setPricingEngine(engine)
assert abs(matching.NPV() - built.NPV()) < 1e-8
output = dict(quantlib=ql.__version__, schedule=[d.ISO() for d in schedule],
              builder=values(built), explicit=values(explicit),
              coupons=[dict(date=c.date().ISO(), amount=c.amount()) for c in built.coupons()])
node_dates = [today, ql.Date(15, 7, 2026), ql.Date(15, 10, 2026),
              ql.Date(15, 6, 2027), ql.Date(15, 6, 2028), ql.Date(15, 6, 2030)]
rates = [.01, .012, .018, .025, .03, .035]
discounts = [1., .998, .988, .967, .923, .84]
probability = ql.HazardRateCurve(node_dates, rates, dc)
nonflat_discount = ql.DiscountCurve(node_dates, discounts, dc)
output['isda'] = []
for fix_name, fix in [('NoFix', ql.IsdaCdsEngine.NoFix), ('Taylor', ql.IsdaCdsEngine.Taylor)]:
    for forward_name, forwards in [('Flat', ql.IsdaCdsEngine.Flat), ('Piecewise', ql.IsdaCdsEngine.Piecewise)]:
        explicit.setPricingEngine(ql.IsdaCdsEngine(
            ql.DefaultProbabilityTermStructureHandle(probability), .4,
            ql.YieldTermStructureHandle(nonflat_discount), False, fix,
            ql.IsdaCdsEngine.HalfDayBias, forwards))
        output['isda'].append(dict(fix=fix_name, forwards=forward_name, **values(explicit)))
print(json.dumps(output, indent=2, sort_keys=True))
