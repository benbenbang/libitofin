"""QuantLib 1.43 numerical oracle for floating bond bindings."""

import json

import QuantLib as q


def make_case(name, gearings=None, spreads=None, caps=None, floors=None, stub=False, indexed=False, fixing_days=None, ex_coupon=False, historical=False, normal=False):
    calendar = q.NullCalendar()
    today = q.Date(15, 8, 2024) if historical else q.Date(10, 1, 2025) if ex_coupon else q.Date(15, 1, 2024)
    q.Settings.instance().evaluationDate = today
    q.Settings.instance().includeReferenceDateEvents = False
    q.Settings.instance().includeTodaysCashFlows = None
    if indexed:
        q.IborCoupon.createIndexedCoupons()
    else:
        q.IborCoupon.createAtParCoupons()
    counter = q.Actual365Fixed()
    rate = q.SimpleQuote(0.03)
    discount = q.SimpleQuote(0.04)
    forwarding = q.FlatForward(q.Date(15, 1, 2024), q.QuoteHandle(rate), counter)
    discounting = q.FlatForward(q.Date(15, 1, 2024), q.QuoteHandle(discount), counter)
    index = q.IborIndex("BondBindingOracle", q.Period(6, q.Months), 2, q.USDCurrency(), calendar, q.ModifiedFollowing, False, q.Actual360(), q.YieldTermStructureHandle(forwarding))
    index.clearFixings()
    start = q.Date(15, 7, 2024)
    end = q.Date(15, 10, 2026) if stub else q.Date(15, 7, 2026)
    schedule = q.Schedule(start, end, q.Period(q.Semiannual), calendar, q.Unadjusted, q.Unadjusted, q.DateGeneration.Backward, False)
    cap_values = [] if caps is None else [q.nullDouble() if v is None else v for v in caps]
    floor_values = [] if floors is None else [q.nullDouble() if v is None else v for v in floors]
    bond = q.FloatingRateBond(2, 1000.0, schedule, index, q.Actual360(), q.Following, q.nullInt() if fixing_days is None else fixing_days, gearings or [], spreads or [], cap_values, floor_values, False, 100.0, start, q.Period(7, q.Days) if ex_coupon else q.Period(), calendar, q.Unadjusted, False, q.Preceding)
    if historical or ex_coupon:
        index.addFixing(q.Date(13, 7, 2024), 0.0275)
    volatility = q.SimpleQuote(0.01 if normal else 0.2)
    surface = q.ConstantOptionletVolatility(q.Date(15, 1, 2024), calendar, q.Following, q.QuoteHandle(volatility), counter, q.Normal if normal else q.ShiftedLognormal)
    pricer = q.BlackIborCouponPricer(q.OptionletVolatilityStructureHandle(surface))
    q.setCouponPricer(bond.cashflows(), pricer)
    bond.setPricingEngine(q.DiscountingBondEngine(q.YieldTermStructureHandle(discounting)))

    def snapshot():
        return {"npv": bond.NPV(), "settlement_value": bond.settlementValue(), "clean": bond.cleanPrice(), "dirty": bond.dirtyPrice(), "accrued": bond.accruedAmount(), "cashflows": [f.amount() for f in bond.cashflows()]}

    result = {"name": name, "today": today.serialNumber(), "start": start.serialNumber(), "end": end.serialNumber(), "gearings": gearings or [], "spreads": spreads or [], "caps": caps or [], "floors": floors or [], "indexed": indexed, "fixing_days": fixing_days, "ex_coupon": ex_coupon, "historical": historical, "normal": normal, "initial": snapshot()}
    rate.setValue(0.037)
    discount.setValue(0.045)
    volatility.setValue(0.015 if normal else 0.3)
    result["updated"] = snapshot()
    return result



def calendar_arithmetic_case():
    today = q.Date(24, 12, 2014)
    q.Settings.instance().evaluationDate = today
    q.Settings.instance().includeReferenceDateEvents = False
    q.Settings.instance().includeTodaysCashFlows = None
    q.IborCoupon.createAtParCoupons()
    dc = q.Actual360()
    forward = q.FlatForward(today, 0.043, dc)
    discount = q.FlatForward(today, 0.023, dc)
    index = q.CustomIborIndex(
        "CalendarArithmeticOracle", q.Period(3, q.Months), 3, q.EURCurrency(),
        q.Japan(), q.NullCalendar(), q.Indonesia(), q.Unadjusted, False, dc,
        q.YieldTermStructureHandle(forward),
    )
    schedule = q.Schedule(
        q.Date(30, 12, 2014), q.Date(30, 3, 2015), q.Period(3, q.Months),
        q.NullCalendar(), q.Unadjusted, q.Unadjusted, q.DateGeneration.Backward, False,
    )
    bond = q.FloatingRateBond(
        0, 375.0, schedule, index, dc, q.Unadjusted, 2, [1.25], [0.002],
        redemption=101.0,
    )
    bond.setPricingEngine(q.DiscountingBondEngine(q.YieldTermStructureHandle(discount)))
    return {"amount": bond.cashflows()[0].amount(), "npv": bond.NPV(), "dirty": bond.dirtyPrice()}

cases = [
    make_case("plain"),
    make_case("varying", spreads=[0.001, 0.002, -0.001]),
    make_case("positive_gearing", gearings=[1.5], spreads=[0.002]),
    make_case("negative_gearing", gearings=[-1.0], spreads=[0.07], caps=[0.04], floors=[0.005]),
    make_case("zero_gearing", gearings=[0.0], spreads=[0.01], caps=[0.008], floors=[0.005]),
    make_case("cap", caps=[0.032]),
    make_case("floor", floors=[0.034]),
    make_case("collar", caps=[0.04], floors=[0.032]),
    make_case("mixed_absent", caps=[None, 0.035, None], floors=[None, None, 0.028]),
    make_case("all_absent", caps=[None]),
    make_case("normal_cap", caps=[0.032], normal=True),
    make_case("stub_at_par", stub=True),
    make_case("stub_indexed", stub=True, indexed=True),
    make_case("explicit_zero_fixing_days", fixing_days=0),
    make_case("historical", historical=True),
    make_case("ex_coupon", ex_coupon=True),
]
print(json.dumps({"quantlib_version": q.__version__, "cases": cases, "calendar_arithmetic": calendar_arithmetic_case()}, sort_keys=True, indent=2))
