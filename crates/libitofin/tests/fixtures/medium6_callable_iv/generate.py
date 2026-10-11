import csv
import math
from pathlib import Path
import QuantLib as q

q.Settings.instance().evaluationDate = q.Date(12, 10, 2026)
reference = q.Date(12, 10, 2026)
issue = q.Date(15, 1, 2026)
maturity = q.Date(15, 1, 2033)
exercise = q.Date(15, 4, 2029)
dc = q.Actual365Fixed()
schedule = q.Schedule(issue, maturity, q.Period(q.Semiannual), q.NullCalendar(),
                      q.Unadjusted, q.Unadjusted, q.DateGeneration.Backward, False)
path = Path(__file__).with_name('reference.tsv')
with path.open('w', newline='') as output:
    writer = csv.writer(output, delimiter='\t', lineterminator='\n')
    writer.writerow(['product', 'direction', 'face', 'lag', 'curve', 'vol',
                     'npv', 'dirty', 'clean', 'native_iv_clean', 'native_iv_dirty', 'corrected_dirty', 'corrected_clean',
                     'corrected_iv_clean', 'corrected_iv_dirty'])
    for product in ['fixed', 'zero']:
        for direction in [q.Callability.Call, q.Callability.Put]:
            for face, lag, curve_kind, vol in [(100., 0, 'flat', .3), (100., 2, 'flat', .12),
                                              (1234567.89, 17, 'nonflat', .3),
                                              (1000., 7, 'nonflat', .7)]:
                term = q.FlatForward(reference, .035, dc) if curve_kind == 'flat' else q.ZeroCurve(
                    [reference, q.Date(12, 10, 2027), q.Date(12, 10, 2030), q.Date(12, 10, 2035)],
                    [.015, .031, .024, .04], dc)
                curve = q.YieldTermStructureHandle(term)
                calls = [q.Callability(q.BondPrice(100. if product == 'fixed' else 85., q.BondPrice.Clean),
                                      direction, exercise)]
                bond = q.CallableFixedRateBond(lag, face, schedule, [.05], dc, q.Unadjusted,
                                               100., issue, calls) if product == 'fixed' else q.CallableZeroCouponBond(
                    lag, face, q.NullCalendar(), maturity, dc, q.Unadjusted, 100., issue, calls)
                bond.setPricingEngine(q.BlackCallableFixedRateBondEngine(q.QuoteHandle(q.SimpleQuote(vol)), curve))
                clean, dirty = bond.cleanPrice(), bond.dirtyPrice()
                iv = [bond.impliedVolatility(q.BondPrice(value, kind), curve, 1.e-10, 300, 0., 1.)
                      for value, kind in [(clean, q.BondPrice.Clean), (dirty, q.BondPrice.Dirty)]]
                settle = bond.settlementDate()
                leg = bond.cashflows()
                value = q.CashFlows.npv(leg, term, False, settle, settle)
                income = sum(flow.amount() * term.discount(flow.date()) for flow in leg[:-1]
                             if settle < flow.date() <= exercise) / term.discount(settle)
                forward = (value - income) * term.discount(settle) / term.discount(exercise)
                fwd_npv = q.CashFlows.npv(leg, term, False, exercise, exercise)
                frequency = q.Semiannual if product == 'fixed' else q.Annual
                fwd_yield = q.CashFlows.yieldRate(leg, fwd_npv, dc, q.Compounded, frequency,
                                                False, exercise)
                duration = q.CashFlows.duration(leg, fwd_yield, dc, q.Compounded, frequency,
                                               q.Duration.Modified, False, exercise)
                strike = (calls[0].price().amount() + bond.accruedAmount(exercise)) / 100. * face
                discount = term.discount(exercise) / term.discount(settle)
                sign = -1. if direction == q.Callability.Call else 1.
                option_type = q.Option.Call if direction == q.Callability.Call else q.Option.Put
                def corrected_value(volatility):
                    stddev = volatility * duration * fwd_yield * math.sqrt(dc.yearFraction(reference, exercise))
                    return value + sign * q.blackFormula(option_type, strike, forward, stddev) * discount
                corrected_dirty = corrected_value(vol) / face * 100.
                corrected_clean = corrected_dirty - bond.accruedAmount()
                corrected_iv = [q.Brent().solve(lambda x: corrected_value(x) - target / 100. * face,
                                               1.e-10, .5, 0., 1.)
                                for target in [corrected_clean + bond.accruedAmount(), corrected_dirty]]
                writer.writerow([product, 'call' if direction == q.Callability.Call else 'put',
                                 repr(face), lag, curve_kind, repr(vol), repr(bond.NPV()),
                                 repr(dirty), repr(clean), repr(iv[0]), repr(iv[1]),
                                 repr(corrected_dirty), repr(corrected_clean),
                                 repr(corrected_iv[0]), repr(corrected_iv[1])])
print(f'QuantLib {q.__version__}: wrote {path}')
