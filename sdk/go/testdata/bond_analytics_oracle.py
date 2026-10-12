"""Independent QuantLib 1.43 bond and leg analytics fixtures."""

import json

import QuantLib as q


def case(name, kind, mode, frequency, rate=0.052, spread=0.007, face=375.0, days=None):
    reference = q.Date(1, 6, 2026)
    settlement = q.Date(26, 6, 2026)
    start = q.Date(24, 3, 2026)
    end = q.Date(24, 3, 2028)
    q.Settings.instance().evaluationDate = q.Date(19, 6, 2026)
    q.Settings.instance().includeReferenceDateEvents = False
    q.Settings.instance().includeTodaysCashFlows = None
    if days is not None:
        start = settlement
        end = settlement + days
    if kind == "ex_coupon":
        settlement = q.Date(20, 9, 2026)
    if kind == "stub":
        start = q.Date(8, 4, 2026)
    calendar = q.NullCalendar()
    dc = q.Actual360()
    curve_dc = q.Actual365Fixed()
    curve = q.FlatForward(reference, 0.032, curve_dc)
    if kind == "zero":
        bond = q.ZeroCouponBond(5, calendar, face, end, q.Unadjusted, 102.0, start)
    else:
        schedule = q.Schedule(start, end, q.Period(q.Semiannual), calendar, q.Unadjusted, q.Unadjusted, q.DateGeneration.Backward, False)
        ex = q.Period(7, q.Days) if kind == "ex_coupon" else q.Period()
        bond = q.FixedRateBond(5, face, schedule, [0.047, 0.058], dc, q.Unadjusted, 102.0, start, calendar, ex, calendar, q.Unadjusted, False)
    compounded = getattr(q, mode)
    native_frequency = getattr(q, frequency)
    clean_curve = q.BondFunctions.cleanPrice(bond, curve, settlement)
    dirty_curve = q.BondFunctions.dirtyPrice(bond, curve, settlement)
    clean_yield = q.BondFunctions.cleanPrice(bond, rate, dc, compounded, native_frequency, settlement)
    dirty_yield = clean_yield + bond.accruedAmount(settlement)
    clean_spread = q.BondFunctions.cleanPrice(bond, curve, spread, compounded, native_frequency, settlement)
    dirty_spread = q.BondFunctions.dirtyPrice(bond, curve, spread, compounded, native_frequency, settlement)
    npv_date = settlement - 9
    leg_npv = q.CashFlows.npv(bond.cashflows(), curve, spread, compounded, native_frequency, False, settlement, npv_date)
    yield_clean = q.BondFunctions.bondYield(bond, q.BondPrice(clean_yield, q.BondPrice.Clean), dc, compounded, native_frequency, settlement, 1e-12, 1000, 0.05)
    yield_dirty = q.BondFunctions.bondYield(bond, q.BondPrice(dirty_yield, q.BondPrice.Dirty), dc, compounded, native_frequency, settlement, 1e-12, 1000, 0.05)
    spread_clean = q.BondFunctions.zSpread(bond, q.BondPrice(clean_spread, q.BondPrice.Clean), curve, compounded, native_frequency, settlement, 1e-12, 1000, 0.0)
    spread_dirty = q.BondFunctions.zSpread(bond, q.BondPrice(dirty_spread, q.BondPrice.Dirty), curve, compounded, native_frequency, settlement, 1e-12, 1000, 0.0)
    leg_spread = q.CashFlows.zSpread(bond.cashflows(), leg_npv, curve, compounded, native_frequency, False, settlement, npv_date, 1e-12, 1000, 0.0)
    return {"name": name, "kind": kind, "compounding": int(compounded), "frequency": (q.Annual, q.Semiannual, q.Quarterly, q.Monthly).index(native_frequency), "face": face, "start": start.serialNumber(), "end": end.serialNumber(), "reference": reference.serialNumber(), "settlement": settlement.serialNumber(), "npv_date": npv_date.serialNumber(), "yield": rate, "spread": spread, "expected": {"clean_curve": clean_curve, "dirty_curve": dirty_curve, "clean_yield": clean_yield, "dirty_yield": dirty_yield, "clean_spread": clean_spread, "dirty_spread": dirty_spread, "yield_clean": yield_clean, "yield_dirty": yield_dirty, "spread_clean": spread_clean, "spread_dirty": spread_dirty, "leg_npv": leg_npv, "leg_spread": leg_spread, "accrued": bond.accruedAmount(settlement)}}


def floating_case():
    q.Settings.instance().evaluationDate = q.Date(15, 1, 2024)
    q.Settings.instance().includeReferenceDateEvents = False
    q.Settings.instance().includeTodaysCashFlows = None
    q.IborCoupon.createAtParCoupons()
    cal = q.NullCalendar()
    dc = q.Actual365Fixed()
    reference = q.Date(15, 1, 2024)
    start, end = q.Date(15, 7, 2024), q.Date(15, 7, 2026)
    forward_quote = q.SimpleQuote(0.03)
    vol_quote = q.SimpleQuote(0.2)
    forward = q.FlatForward(reference, q.QuoteHandle(forward_quote), dc)
    engine_curve = q.FlatForward(reference, 0.04, dc)
    analytics_curve = q.FlatForward(reference, 0.051, dc)
    index = q.IborIndex("AnalyticsFloatingOracle", q.Period(6, q.Months), 2, q.USDCurrency(), cal, q.ModifiedFollowing, False, q.Actual360(), q.YieldTermStructureHandle(forward))
    index.clearFixings()
    schedule = q.Schedule(start, end, q.Period(q.Semiannual), cal, q.Unadjusted, q.Unadjusted, q.DateGeneration.Backward, False)
    bond = q.FloatingRateBond(2, 1000.0, schedule, index, q.Actual360(), caps=[0.032], issueDate=start)
    surface = q.ConstantOptionletVolatility(reference, cal, q.Following, q.QuoteHandle(vol_quote), dc)
    pricer = q.BlackIborCouponPricer(q.OptionletVolatilityStructureHandle(surface))
    q.setCouponPricer(bond.cashflows(), pricer)
    bond.setPricingEngine(q.DiscountingBondEngine(q.YieldTermStructureHandle(engine_curve)))
    cached = bond.NPV()
    bond.freeze()
    settlement = bond.settlementDate()

    def values():
        clean_curve = q.BondFunctions.cleanPrice(bond, analytics_curve, settlement)
        dirty_curve = q.BondFunctions.dirtyPrice(bond, analytics_curve, settlement)
        clean_yield = q.BondFunctions.cleanPrice(bond, 0.047, dc, q.Compounded, q.Semiannual, settlement)
        clean_spread = q.BondFunctions.cleanPrice(bond, analytics_curve, 0.006, q.Compounded, q.Semiannual, settlement)
        return {"clean_curve": clean_curve, "dirty_curve": dirty_curve, "clean_yield": clean_yield, "dirty_yield": clean_yield + bond.accruedAmount(settlement), "clean_spread": clean_spread, "dirty_spread": q.BondFunctions.dirtyPrice(bond, analytics_curve, 0.006, q.Compounded, q.Semiannual, settlement), "leg_npv": q.CashFlows.npv(bond.cashflows(), analytics_curve, 0.006, q.Compounded, q.Semiannual, False, settlement, settlement), "cached_npv": bond.NPV()}

    initial = values()
    forward_quote.setValue(0.037)
    vol_quote.setValue(0.3)
    return {"initial": initial, "updated": values(), "cached_npv": cached, "settlement": settlement.serialNumber()}


def inclusion_cases():
    reference = q.Date(1, 6, 2026)
    settlement = q.Date(24, 3, 2028)
    curve = q.FlatForward(reference, 0.032, q.Actual365Fixed())
    leg = q.Leg([q.SimpleCashFlow(382.5, settlement)])
    rows = []
    for today in (None, False, True):
        for reference_flag in (False, True):
            for include in (None, False, True):
                settings = q.Settings.instance()
                settings.evaluationDate = settlement
                settings.includeTodaysCashFlows = today
                settings.includeReferenceDateEvents = reference_flag
                value = q.CashFlows.npv(leg, curve, 0.007, q.Compounded, q.Semiannual, include, settlement, settlement - 9)
                spread = q.CashFlows.zSpread(leg, value, curve, q.Compounded, q.Semiannual, include, settlement, settlement - 9, 1e-12, 1000, 0.0)
                rows.append({"today": today, "reference": reference_flag, "include": include, "npv": value, "spread": spread})
    return rows


cases = []
for mode in ("Simple", "Compounded", "Continuous", "SimpleThenCompounded", "CompoundedThenSimple"):
    for frequency in ("Annual", "Semiannual", "Quarterly", "Monthly"):
        cases.append(case(mode + "_" + frequency, "fixed", mode, frequency))
    cases.append(case("negative_" + mode, "fixed", mode, "Semiannual", rate=-0.012, spread=-0.009))
for kind in ("zero", "stub", "ex_coupon"):
    for face in (250.0, 1500.0):
        cases.append(case(kind + "_" + str(face), kind, "Compounded", "Semiannual", face=face))
for mode in ("SimpleThenCompounded", "CompoundedThenSimple"):
    for frequency, count in (("Annual", 360), ("Semiannual", 180), ("Quarterly", 90), ("Monthly", 30)):
        for offset in (-1, 0, 1):
            cases.append(case("threshold_" + mode + "_" + frequency + "_" + str(offset), "zero", mode, frequency, days=count + offset))
print(json.dumps({"quantlib_version": q.__version__, "cases": cases, "floating": floating_case(), "inclusion": inclusion_cases()}, sort_keys=True, indent=2))
