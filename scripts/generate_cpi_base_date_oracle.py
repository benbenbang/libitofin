"""Generate compact base-date CPI coupon fixtures using an independent QuantLib wheel."""
import csv
import hashlib
import json
import math
import sys
from pathlib import Path

import QuantLib as q
import QuantLib._QuantLib as native

SOURCE_REVISION = '9863b578af0caa4cecabf697196533e84a8308b6'
TODAY = q.Date(18, 8, 2025)
DC = q.Actual365Fixed()
FIXINGS = [(y, m, 208.0 + (y - 2022) * 9.1 + m * 0.73 + (m % 3) * 0.17)
           for y in range(2022, 2026) for m in range(1, 13) if (y, m) <= (2025, 7)]

OUTPUT = Path(sys.argv[1]) if len(sys.argv) > 1 else Path('sdk/go/testdata')


def result(fn):
    try:
        x = fn()
    except RuntimeError as e:
        return dict(kind='error', error=str(e))
    if math.isnan(x):
        return dict(kind='nan')
    if math.isinf(x):
        return dict(kind='positive_infinity' if x > 0 else 'negative_infinity')
    return dict(kind='finite', value=x)


def provenance():
    binary = Path(native.__file__)
    return dict(native_version=q.__version__, native_binary=str(binary),
                native_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                source_reference_revision=SOURCE_REVISION, source_is_wheel_build_provenance=False,
                generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest())


def write(name, values):
    OUTPUT.mkdir(parents=True, exist_ok=True)
    values['provenance'] = provenance()
    raw = json.dumps(values, indent=2, sort_keys=True, allow_nan=False) + '\n'
    inputs = values['inputs'].copy()
    fixings = inputs.pop('historical_fixings')
    metadata = dict(inputs=inputs, provenance=values['provenance'],
                    qualifications=values['qualifications'],
                    full_native_matrix_sha256=hashlib.sha256(raw.encode()).hexdigest(),
                    case_count=len(values['cases']), stages=['original', 'overwritten', 'relinked'])
    (OUTPUT / 'cpi-base-date-meta.json').write_text(json.dumps(metadata, indent=2, sort_keys=True) + '\n')
    with (OUTPUT / 'cpi-base-date-fixings.tsv').open('w', newline='') as handle:
        writer = csv.writer(handle, delimiter='\t', lineterminator='\n')
        writer.writerow(['date', 'value'])
        writer.writerows((row['date'], repr(row['value'])) for row in fixings)
    fields = ['id', 'stage', 'mode', 'interpolation', 'lag_months', 'fixed_rate', 'nominal',
              'base_date', 'start_date', 'end_date', 'payment_date', 'ex_coupon_date',
              'base_overwrite_date', 'base_overwrite_value', 'amount', 'rate', 'base_fixing',
              'index_fixing', 'index_ratio', 'accrual_period']
    fields += [f'accrued_{i}_{kind}' for i in range(8) for kind in ['date', 'value']]
    def scalar(value):
        if value['kind'] != 'finite':
            raise ValueError(f'non-finite native fixture: {value}')
        return repr(value['value'])
    with (OUTPUT / 'cpi-base-date-coupons.tsv').open('w', newline='') as handle:
        writer = csv.DictWriter(handle, fieldnames=fields, delimiter='\t', lineterminator='\n')
        writer.writeheader()
        for case in values['cases']:
            for stage in metadata['stages']:
                row = {key: case[key] for key in fields[:14] if key != 'stage'}
                row['stage'] = stage
                result = case[stage]
                row.update({key: scalar(result[key]) for key in fields[14:19]})
                row['accrual_period'] = repr(result['accrual_period'])
                for i, accrued in enumerate(result['accrued']):
                    row[f'accrued_{i}_date'] = accrued['date']
                    row[f'accrued_{i}_value'] = scalar(accrued['value'])
                writer.writerow(row)
    print('wrote 36 native cases x 3 stages and complete inputs to', OUTPUT)


def setup():
    q.IndexManager.instance().clearHistories()
    q.Settings.instance().evaluationDate = TODAY
    q.Settings.instance().includeTodaysCashFlows = None
    q.Settings.instance().includeReferenceDateEvents = False
    handle = q.RelinkableZeroInflationTermStructureHandle()
    handle.linkTo(q.ZeroInflationCurve(TODAY, [q.Date(1, 1, 2025), q.Date(1, 1, 2035)],
                                     [-.0075, -.0075], q.Monthly, DC))
    index = q.UKRPI(handle)
    for year, month, value in FIXINGS:
        index.addFixing(q.Date(1, month, year), value, True)
    return index, handle


def inflation_inputs():
    return dict(evaluation_date=TODAY.ISO(), day_counter='Actual365Fixed',
                inflation_curve_dates=['2025-01-01','2035-01-01'], inflation_rate=-.0075,
                relinked_inflation_rate=.065, historical_fixings=[dict(date=f'{y:04}-{m:02}-01',value=v) for y,m,v in FIXINGS])


def base_coupon_cases():
    cases = []
    for interpolation, label in [(q.CPI.Flat, 'flat'), (q.CPI.Linear, 'linear')]:
        for lag in [0,1,3]:
            for rate in [-.0125,0.0,.0375]:
                for mode in ['historical','forecast']:
                    index, handle = setup()
                    base = q.Date(31,1,2024)
                    start = q.Date(17,3,2024) if mode=='historical' else q.Date(17,9,2025)
                    end = q.Date(17,9,2024) if mode=='historical' else q.Date(17,3,2026)
                    pay, ex = end+3,end-5
                    period=q.Period(lag,q.Months)
                    coupon=q.CPICoupon(base,pay,8123.75,start,end,index,period,interpolation,DC,rate,q.Date(),q.Date(),ex)
                    coupon.setPricer(q.CPICouponPricer())
                    base_fn=lambda:q.CPI.laggedFixing(index,base+period,period,interpolation)
                    def values():
                        return dict(amount=result(coupon.amount),rate=result(coupon.rate),base_fixing=result(base_fn),
                                    index_fixing=result(coupon.indexFixing),index_ratio=result(lambda:coupon.indexRatio(end)),
                                    accrual_period=coupon.accrualPeriod(),
                                    accrued=[dict(date=d.ISO(),value=result(lambda:coupon.accruedAmount(d))) for d in [start,start+1,end-7,ex,end-1,end,pay,pay+1]])
                    original=values()
                    observation_base=(base+period)-period
                    change=q.Date(1,observation_base.month(),observation_base.year())
                    index.addFixing(change,180.25,True)
                    overwritten=values()
                    handle.linkTo(q.ZeroInflationCurve(TODAY,[q.Date(1,1,2025),q.Date(1,1,2035)],[.065,.065],q.Monthly,DC))
                    relinked=values()
                    cases.append(dict(id=len(cases),interpolation=label,lag_months=lag,fixed_rate=rate,mode=mode,
                                      base_date=base.ISO(),start_date=start.ISO(),end_date=end.ISO(),payment_date=pay.ISO(),ex_coupon_date=ex.ISO(),
                                      nominal=8123.75,base_overwrite_date=change.ISO(),base_overwrite_value=180.25,
                                      original=original,overwritten=overwritten,relinked=relinked))
    write('base-coupons-native-v1.json',dict(inputs=inflation_inputs(),cases=cases,
          qualifications=['Native coupon base CPI observed via laggedFixing(baseDate+lag,lag,interpolation), not simply laggedFixing(baseDate,0,interpolation).',
                          'Month-end date arithmetic is not invertible; 31January plus1month minus1month may be29January.']))


if __name__ == '__main__':
    base_coupon_cases()
