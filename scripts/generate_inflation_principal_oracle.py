"""Generate standalone principal fixtures with the compiled QuantLib 1.43 oracle."""
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

OUTPUT = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parents[1] / 'sdk/go/testdata'


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
    phases = ['original', 'relinked_unmodified']
    measures = ['amount', 'base_fixing', 'index_fixing']
    columns = ['id', 'kind', 'interpolation', 'lag_months', 'growth_only', 'mode',
               'nominal', 'start_date', 'observation_date', 'payment_date', 'base_cpi']
    columns += [phase + '_' + measure for phase in phases for measure in measures]
    path = OUTPUT / 'inflation-principal-native.tsv'
    with path.open('w', newline='') as stream:
        writer = csv.DictWriter(stream, fieldnames=columns, delimiter='\t', lineterminator='\n')
        writer.writeheader()
        for case in values['cases']:
            row = {key: case[key] for key in columns if key in case}
            row['growth_only'] = str(case['growth_only']).lower()
            for phase in phases:
                for measure in measures:
                    native_result = case[phase][measure]
                    if native_result['kind'] != 'finite':
                        raise RuntimeError(f'non-finite native result: {case["id"]} {phase} {measure}')
                    row[phase + '_' + measure] = repr(native_result['value'])
            writer.writerow(row)
    metadata = dict(inputs=values['inputs'], provenance=provenance(), case_count=len(values['cases']),
                    fixture_sha256=hashlib.sha256(path.read_bytes()).hexdigest())
    metadata['provenance'].pop('native_binary')
    (OUTPUT / 'inflation-principal-inputs.json').write_text(json.dumps(metadata, indent=2, sort_keys=True) + '\n')
    print(path, metadata['fixture_sha256'])


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


def inflation_cases():
    cases = []
    nominal = 8123.75
    for interpolation, label in [(q.CPI.Flat, 'flat'), (q.CPI.Linear, 'linear')]:
        for lag in [0, 1, 3]:
            for growth_only in [False, True]:
                for mode in ['historical', 'forecast']:
                    index, handle = setup()
                    start = q.Date(31, 1, 2024)
                    end = q.Date(31, 7, 2024) if mode == 'historical' else q.Date(31, 1, 2026)
                    pay = end + 3
                    period = q.Period(lag, q.Months)
                    for kind in ['explicit', 'live_base_date']:
                        index, handle = setup()
                        explicit_base = 219.875
                        if kind == 'explicit':
                            flow = q.CPICashFlow(nominal, index, q.Date(), explicit_base, end, period, interpolation, pay, growth_only)
                            base_fn = lambda: explicit_base
                        else:
                            flow = q.ZeroInflationCashFlow(nominal, index, interpolation, start, end, period, pay, growth_only)
                            base_fn = lambda: q.CPI.laggedFixing(index, start, period, interpolation)
                        end_fn = lambda: q.CPI.laggedFixing(index, end, period, interpolation)
                        original = dict(amount=result(flow.amount), base_fixing=result(base_fn), index_fixing=result(end_fn))
                        handle.linkTo(q.ZeroInflationCurve(TODAY, [q.Date(1, 1, 2025), q.Date(1, 1, 2035)],
                                                         [.065, .065], q.Monthly, DC))
                        relinked_unmodified = dict(amount=result(flow.amount), base_fixing=result(base_fn), index_fixing=result(end_fn))
                        handle.linkTo(q.ZeroInflationCurve(TODAY, [q.Date(1, 1, 2025), q.Date(1, 1, 2035)],
                                                         [-.0075, -.0075], q.Monthly, DC))
                        base_date = start - period
                        index.addFixing(q.Date(1, base_date.month(), base_date.year()), 180.25, True)
                        overwrite = dict(amount=result(flow.amount), base_fixing=result(base_fn), index_fixing=result(end_fn))
                        handle.linkTo(q.ZeroInflationCurve(TODAY, [q.Date(1, 1, 2025), q.Date(1, 1, 2035)],
                                                         [.065, .065], q.Monthly, DC))
                        relinked = dict(amount=result(flow.amount), base_fixing=result(base_fn), index_fixing=result(end_fn))
                        cases.append(dict(id=len(cases), kind=kind, interpolation=label, lag_months=lag,
                                          growth_only=growth_only, mode=mode, nominal=nominal,
                                          start_date=start.ISO(), observation_date=end.ISO(), payment_date=pay.ISO(),
                                          base_cpi=explicit_base, base_overwrite_date=q.Date(1,base_date.month(),base_date.year()).ISO(),
                                          base_overwrite_value=180.25, original=original, relinked_unmodified=relinked_unmodified, overwritten=overwrite, relinked=relinked))
    write('inflation-flows-native-v3.json', dict(inputs=inflation_inputs(), cases=cases))


if __name__ == '__main__':
    inflation_cases()
