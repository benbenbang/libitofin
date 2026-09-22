"""Independent QuantLib 1.43 oracles for the #577 Python and Go facades.

Run with QuantLib's Python package; no itofin imports or generated Rust values.
Normal Ibor fixtures complement the C++ smile/Stripper2 oracle. The overnight
fixture uses actual OvernightLeg cash flows, not an Ibor schedule substitute.
"""

import QuantLib as q

assert q.__version__ == "1.43", q.__version__
q.Settings.instance().evaluationDate = q.Date(28, 10, 2013)
curve = q.YieldTermStructureHandle(
    q.FlatForward(q.Date(28, 10, 2013), .04, q.Actual365Fixed())
)
tenors = [q.Period(n, q.Years) for n in (1, 2, 3, 5, 7, 10)]
strikes = [.01, .02, .04, .06, .1]
vols = [[.008 + .0002 * i + .002 * k for k in strikes]
        for i in range(len(tenors))]
surface = q.CapFloorTermVolSurface(
    0, q.TARGET(), q.Following, tenors, strikes, vols, q.Actual365Fixed()
)
index = q.Euribor6M(curve)
stripper = q.OptionletStripper1(surface, index, type=q.Normal, accuracy=1e-10)
adapter = q.StrippedOptionletAdapter(stripper)
adapter.enableExtrapolation()
print("normal_switch", format(stripper.switchStrike(), ".17g"))
for time in (2., 4., 6.):
    for strike in (.02, .035, .06):
        print("normal_vol", time, strike, format(adapter.volatility(time, strike), ".17g"))
for years, strike in ((1, .02), (3, .04), (10, .06)):
    cap = q.MakeCapFloor(q.CapFloor.Cap, q.Period(years, q.Years), index,
                        strike, q.Period(0, q.Days))
    cap.setPricingEngine(q.BachelierCapFloorEngine(
        curve, q.OptionletVolatilityStructureHandle(adapter)))
    print("normal_cap", years, strike, format(cap.NPV(), ".17g"))

overnight = q.Eonia(curve)
overnight_stripper = q.OptionletStripper1(
    surface, overnight, type=q.Normal, accuracy=1e-10,
    optionletFrequency=q.Period(6, q.Months),
)
overnight_adapter = q.StrippedOptionletAdapter(overnight_stripper)
overnight_adapter.enableExtrapolation()
schedule = q.Schedule(
    q.Date(28, 10, 2014), q.Date(28, 10, 2016), q.Period(6, q.Months),
    q.TARGET(), q.ModifiedFollowing, q.ModifiedFollowing,
    q.DateGeneration.Backward, False,
)
leg = q.OvernightLeg([1.], schedule, overnight,
                     paymentConvention=q.ModifiedFollowing, paymentLag=2)
cap = q.Cap(leg, [.04])
cap.setPricingEngine(q.BachelierCapFloorEngine(
    curve, q.OptionletVolatilityStructureHandle(overnight_adapter)))
print("overnight_stripped_cap", format(cap.NPV(), ".17g"))
flat = q.ConstantOptionletVolatility(
    q.Date(28, 10, 2013), q.TARGET(), q.Following, .008,
    q.Actual365Fixed(), q.Normal,
)
cap.setPricingEngine(q.BachelierCapFloorEngine(
    curve, q.OptionletVolatilityStructureHandle(flat)))
print("overnight_constant_cap", format(cap.NPV(), ".17g"))
