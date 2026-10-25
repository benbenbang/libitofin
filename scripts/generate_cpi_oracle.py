import hashlib
import json
import platform
import sys
from pathlib import Path

import QuantLib as q
import QuantLib._QuantLib as native

SOURCE_REVISION = '9863b578af0caa4cecabf697196533e84a8308b6'
EVALUATION = q.Date(15, 6, 2025)
BASE_DATE = q.Date(1, 1, 2025)
FIXINGS = [
    (2024, 10, 99.5), (2024, 11, 99.8), (2024, 12, 100.0),
    (2025, 1, 100.2), (2025, 2, 100.4), (2025, 3, 100.6),
    (2025, 4, 100.8), (2025, 5, 101.0),
]
DC = q.Actual365Fixed()
LAG = q.Period(3, q.Months)


def date(d):
    return d.ISO() if d != q.Date() else None


def setup(evaluation=EVALUATION):
    q.IndexManager.instance().clearHistories()
    q.Settings.instance().evaluationDate = evaluation
    q.Settings.instance().includeReferenceDateEvents = False
    q.Settings.instance().enforcesTodaysHistoricFixings = False
    z = q.ZeroInflationCurve(EVALUATION, [BASE_DATE, q.Date(1, 1, 2035)], [.025, .025], q.Monthly, DC)
    zh = q.RelinkableZeroInflationTermStructureHandle(z)
    index = q.UKRPI(zh)
    for y, m, v in FIXINGS:
        index.addFixing(q.Date(1, m, y), v, True)
    quote = q.SimpleQuote(.04)
    nominal = q.YieldTermStructureHandle(q.FlatForward(EVALUATION, q.QuoteHandle(quote), DC))
    return index, zh, nominal, quote


def schedule(end=q.Date(15, 1, 2028)):
    return q.Schedule(q.Date(15, 1, 2025), end, q.Period(6, q.Months), q.NullCalendar(),
                      q.Unadjusted, q.Unadjusted, q.DateGeneration.Forward, False)


def coupon_snapshot(c):
    ds = [q.Date(15, 1, 2025), q.Date(16, 1, 2025), q.Date(15, 6, 2025),
          q.Date(9, 7, 2025), q.Date(10, 7, 2025), q.Date(14, 7, 2025),
          q.Date(15, 7, 2025), q.Date(16, 7, 2025)]
    return dict(payment=date(c.date()), fixing_date=date(c.fixingDate()),
                index_fixing=c.indexFixing(), rate=c.rate(), amount=c.amount(),
                accrual_period=c.accrualPeriod(),
                accrued=[dict(date=date(d), amount=c.accruedAmount(d)) for d in ds])


def coupon_rows():
    rows = []
    for interpolation in [q.CPI.Flat, q.CPI.Linear]:
        index, zh, nominal, quote = setup()
        name = 'flat' if interpolation == q.CPI.Flat else 'linear'
        c = q.CPICoupon(100.0, q.Date(15, 7, 2025), 100000.0,
                        q.Date(15, 1, 2025), q.Date(15, 7, 2025), index, LAG,
                        interpolation, DC, .02, q.Date(), q.Date(), q.Date(10, 7, 2025))
        c.setPricer(q.CPICouponPricer())
        rows.append(dict(name=name + '_historical_excoupon', values=coupon_snapshot(c)))
        index.addFixing(q.Date(1, 4, 2025), 101.8, True)
        rows.append(dict(name=name + '_historical_fixing_update', values=coupon_snapshot(c)))
        f = q.CPICoupon(100.0, q.Date(15, 7, 2026), 100000.0,
                        q.Date(15, 1, 2026), q.Date(15, 7, 2026), index, LAG,
                        interpolation, DC, .02)
        f.setPricer(q.CPICouponPricer())
        rows.append(dict(name=name + '_forecast', values=dict(amount=f.amount(), rate=f.rate(),
                    index_fixing=f.indexFixing(), fixing_date=date(f.fixingDate()))))
        zh.linkTo(q.ZeroInflationCurve(EVALUATION, [BASE_DATE, q.Date(1, 1, 2035)],
                                      [.03, .03], q.Monthly, DC))
        rows.append(dict(name=name + '_forecast_relinked_3pct', values=dict(amount=f.amount(),
                    rate=f.rate(), index_fixing=f.indexFixing())))
    return rows


def cashflows(leg):
    return [dict(date=date(c.date()), amount=c.amount()) for c in leg]


def leg_rows():
    rows = []
    for interpolation in [q.CPI.Flat, q.CPI.Linear]:
        for growth_only in [False, True]:
            index, zh, nominal, quote = setup()
            leg = q.CPILeg([100000.0], schedule(), index, 100.0, LAG, DC, q.Unadjusted,
                           [.02], [], [], q.Period(), q.NullCalendar(), q.Unadjusted,
                           False, q.NullCalendar(), growth_only, interpolation)
            rows.append(dict(interpolation='flat' if interpolation == q.CPI.Flat else 'linear',
                        growth_only=growth_only, cashflows=cashflows(leg),
                        npv=q.CashFlows.npv(leg, nominal, False, EVALUATION, EVALUATION)))
    return rows


def bond_snapshot(b):
    return dict(npv=b.NPV(), clean_price=b.cleanPrice(), dirty_price=b.dirtyPrice(),
                accrued_per100=b.accruedAmount(), notional=b.notional(),
                settlement=date(b.settlementDate()), maturity=date(b.maturityDate()),
                cashflows=cashflows(b.cashflows()))


def bond_rows():
    rows = []
    for interpolation in [q.CPI.Flat, q.CPI.Linear]:
        index, zh, nominal, quote = setup()
        b = q.CPIBond(2, 100000.0, False, 100.0, LAG, index, interpolation, schedule(),
                      [.02], DC, q.Unadjusted, q.Date(15, 1, 2025), q.NullCalendar())
        b.setPricingEngine(q.DiscountingBondEngine(nominal))
        label = 'flat' if interpolation == q.CPI.Flat else 'linear'
        rows.append(dict(name=label, values=bond_snapshot(b)))
        quote.setValue(.05)
        rows.append(dict(name=label + '_discount_quote_5pct', values=bond_snapshot(b)))
        index.addFixing(q.Date(1, 4, 2025), 101.8, True)
        rows.append(dict(name=label + '_fixing_updated', values=bond_snapshot(b)))
        zh.linkTo(q.ZeroInflationCurve(EVALUATION, [BASE_DATE, q.Date(1, 1, 2035)],
                                      [.03, .03], q.Monthly, DC))
        rows.append(dict(name=label + '_inflation_relinked_3pct', values=bond_snapshot(b)))
    index, zh, nominal, quote = setup()
    b = q.CPIBond(2, 100000.0, False, 100.0, LAG, index, q.CPI.Linear,
                  schedule(q.Date(15, 7, 2028)), [.02], DC, q.Following,
                  q.Date(15, 1, 2025), q.WeekendsOnly(), q.Period(5, q.Days),
                  q.WeekendsOnly(), q.Unadjusted, False)
    b.setPricingEngine(q.DiscountingBondEngine(nominal))
    rows.append(dict(name='linear_adjusted_terminal_weekend', values=bond_snapshot(b)))
    q.Settings.instance().evaluationDate = q.Date(10, 7, 2025)
    rows.append(dict(name='linear_negative_excoupon', values=bond_snapshot(b)))
    index, zh, nominal, quote = setup(q.Date(16, 1, 2026))
    principal = q.CPICashFlow(100000.0, index, q.Date(), 100.0, q.Date(15, 1, 2026),
                             LAG, q.CPI.Flat, q.Date(15, 1, 2026), False)
    try:
        principal.amount()
        raise AssertionError('missing principal fixing did not fail')
    except RuntimeError as error:
        rows.append(dict(name='missing_redemption_fixing', date=date(principal.date()),
                         error=str(error)))
    return rows


def swap_snapshot(s):
    return dict(npv=s.NPV(), cpi_leg_npv=s.legNPV(0), floating_leg_npv=s.legNPV(1),
                floating_leg_bps=s.legBPS(1), fair_spread=s.fairSpread())


def swap_rows():
    rows = []
    for interpolation in [q.CPI.Flat, q.CPI.Linear]:
        for growth_only in [False, True]:
            for inflation_nominal in [100000.0, 90000.0]:
                index, zh, nominal, quote = setup()
                floating = q.GBPLibor(q.Period(6, q.Months), nominal)
                floating.addFixing(floating.fixingDate(q.Date(15, 1, 2025)), .042, True)
                values = {}
                for side in [q.Swap.Payer, q.Swap.Receiver]:
                    s = q.CPISwap(side, 100000.0, growth_only, .001, DC, schedule(),
                                  q.Unadjusted, floating.fixingDays(), floating, .02, 100.0,
                                  DC, schedule(), q.Unadjusted, LAG, index, interpolation,
                                  inflation_nominal)
                    s.setPricingEngine(q.DiscountingSwapEngine(nominal))
                    values['payer' if side == q.Swap.Payer else 'receiver'] = swap_snapshot(s)
                rows.append(dict(interpolation='flat' if interpolation == q.CPI.Flat else 'linear',
                                  growth_only=growth_only, inflation_nominal=inflation_nominal,
                                  values=values))
    return rows


def collect():
    binary = Path(native.__file__)
    output = dict(provenance=dict(quantlib_wheel_version=q.__version__,
                  native_module=str(binary), native_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                  python=sys.version, executable=sys.executable, platform=platform.platform(),
                  source_reference_revision=SOURCE_REVISION,
                  source_is_wheel_build_provenance=False,
                  unsupported_wheel_classes=[n for n in ['CPICapFloor', 'CPICapFloorTermPriceSurface',
                      'InterpolatedCPICapFloorTermPriceSurface', 'InterpolatingCPICapFloorEngine']
                      if not hasattr(q, n)]),
                  inputs=dict(evaluation_date=date(EVALUATION), inflation_curve_reference=date(EVALUATION),
                              inflation_curve_dates=[date(BASE_DATE), '2035-01-01'],
                              inflation_curve_rates=[.025, .025], inflation_frequency='Monthly',
                              day_counter='Actual365Fixed', index='UKRPI',
                              historical_fixings=[dict(date=f'{y:04}-{m:02}-01', value=v) for y, m, v in FIXINGS],
                              base_cpi=100.0, nominal=100000.0, fixed_rate=.02,
                              nominal_curve_rate=.04, nominal_curve_compounding='Continuous',
                              observation_lag_months=3, schedule_start='2025-01-15',
                              schedule_end='2028-01-15', schedule_tenor_months=6,
                              schedule_calendar='NullCalendar', schedule_convention='Unadjusted',
                              nominal_day_counter='Actual365Fixed', float_index='GBPLibor6M',
                              float_fixing_days=0, float_historical_fixing=.042, spread=.001,
                              float_index_day_counter='Actual365Fixed',
                              float_index_fixing_calendar='UnitedKingdom::Exchange',
                              float_index_business_convention='ModifiedFollowing',
                              float_index_end_of_month=True),
                  coupons=coupon_rows(), legs=leg_rows(), bonds=bond_rows(), swaps=swap_rows())
    return output


def main():
    output = collect()
    destination = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parents[1] / 'sdk/go/testdata'
    destination.mkdir(parents=True, exist_ok=True)
    provenance = dict(output['provenance'])
    provenance['native_module'] = Path(provenance['native_module']).name
    provenance.pop('executable')
    shared = dict(schema=1, provenance=provenance, inputs=output['inputs'])
    files = {'cpi-oracle-inputs.json': shared}
    for kind in ['coupons', 'legs', 'bonds', 'swaps']:
        files[f'cpi-oracle-{kind}.json'] = dict(schema=1, generator='scripts/generate_cpi_oracle.py',
                                             inputs='cpi-oracle-inputs.json', **{kind: output[kind]})
    for name, value in files.items():
        path = destination / name
        path.write_text(json.dumps(value, indent=2, allow_nan=False, sort_keys=True) + '\n')
        print(name, hashlib.sha256(path.read_bytes()).hexdigest())


if __name__ == '__main__':
    main()
