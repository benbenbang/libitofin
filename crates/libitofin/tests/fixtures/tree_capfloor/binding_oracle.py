import QuantLib as q

assert q.__version__ == "1.43"
q.Settings.instance().evaluationDate = q.Date(15, 1, 2026)
for rate, strike, vol in [
    (0.03, 0.04, 0.01),
    (-0.01, -0.005, 0.007),
    (0.03, 0.04, 0.0),
]:
    curve = q.YieldTermStructureHandle(
        q.FlatForward(q.Date(15, 1, 2026), rate, q.Actual365Fixed())
    )
    index = q.Euribor6M(curve)
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
        [100.0], schedule, index, q.Actual360(), q.Unadjusted, [0], [1.0], [0.0]
    )
    engine = q.BachelierCapFloorEngine(curve, q.QuoteHandle(q.SimpleQuote(vol)))
    for obj in [
        q.Cap(leg, [strike]),
        q.Floor(leg, [strike - 0.015]),
        q.Collar(leg, [strike], [strike - 0.015]),
    ]:
        obj.setPricingEngine(engine)
        print(rate, strike, vol, type(obj).__name__, repr(obj.NPV()), repr(obj.vega()))

curve = q.YieldTermStructureHandle(
    q.FlatForward(q.Date(15, 1, 2026), 0.03, q.Actual365Fixed())
)
index = q.Euribor6M(curve)
leg = q.IborLeg([100.0], schedule, index, q.Actual360(), q.Unadjusted, [0])
for obj in [q.Cap(leg, [0.04]), q.Floor(leg, [0.025]), q.Collar(leg, [0.04], [0.025])]:
    obj.setPricingEngine(q.TreeCapFloorEngine(q.HullWhite(curve, 0.05, 0.01), 100))
    print("tree", repr(obj.NPV()))
for shift in [0.0, 0.01]:
    h = q.CapHelper(
        q.Period(5, q.Years),
        q.QuoteHandle(q.SimpleQuote(0.2)),
        q.Euribor6M(curve),
        q.Annual,
        q.Actual365Fixed(),
        False,
        curve,
        q.BlackCalibrationHelper.RelativePriceError,
        q.ShiftedLognormal,
        shift,
    )
    print("shifted helper", shift, repr(h.marketValue()))
