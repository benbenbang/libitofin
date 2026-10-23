"""Regenerate bond risk fixtures with the separately installed QuantLib 1.43 wheel."""
import csv
import io
import hashlib
import json
from pathlib import Path
import QuantLib as q

R = Path(__file__).resolve().parents[3]
if q.__version__ != '1.43':
    raise RuntimeError('QuantLib 1.43 is required')
source = R / 'sdk/go/testdata/bond_analytics_oracle.json'
original = json.loads(source.read_text())
cal, dc = q.NullCalendar(), q.Actual360()
frequencies = [q.Annual, q.Semiannual, q.Quarterly, q.Monthly]
compoundings = [q.Simple, q.Compounded, q.Continuous, q.SimpleThenCompounded, q.CompoundedThenSimple]
rows = []
for row in original['cases']:
    row = {k: v for k, v in row.items() if k != 'expected'}
    start, end, settlement = (q.Date(row[k]) for k in ['start', 'end', 'settlement'])
    if row['kind'] == 'zero':
        bond = q.ZeroCouponBond(5, cal, row['face'], end, q.Unadjusted, 102., start)
    else:
        schedule = q.Schedule(start, end, q.Period(q.Semiannual), cal, q.Unadjusted,
                              q.Unadjusted, q.DateGeneration.Backward, False)
        bond = q.FixedRateBond(5, row['face'], schedule, [.047, .058], dc, q.Unadjusted,
                               102., start, cal, q.Period(7, q.Days) if row['kind'] == 'ex_coupon' else q.Period(),
                               cal, q.Unadjusted, False)
    y = q.InterestRate(row['yield'], dc, compoundings[row['compounding']], frequencies[row['frequency']])
    expected = {name: q.BondFunctions.duration(bond, y, duration, settlement)
                for name, duration in [('simple', q.Duration.Simple), ('modified', q.Duration.Modified)]}
    expected['macaulay'] = q.BondFunctions.duration(bond, y, q.Duration.Macaulay, settlement) if row['compounding'] == 1 else None
    expected.update({name: getattr(q.BondFunctions, method)(bond, y, settlement)
                     for name, method in [('convexity', 'convexity'), ('basis_point_value', 'basisPointValue'), ('yield_value_basis_point', 'yieldValueBasisPoint')]})
    row['expected'] = expected
    rows.append(row)
columns = ['name', 'kind', 'compounding', 'frequency', 'face', 'start', 'end', 'reference', 'settlement', 'npv_date', 'yield', 'spread', 'simple', 'macaulay', 'modified', 'convexity', 'basis_point_value', 'yield_value_basis_point']
stream = io.StringIO(newline='')
writer = csv.writer(stream, delimiter='\t', lineterminator='\n')
writer.writerow(columns)
for row in rows:
    flat = {**{key: value for key, value in row.items() if key != 'expected'}, **row['expected']}
    writer.writerow(['NA' if flat[key] is None else repr(flat[key]) if isinstance(flat[key], float) else flat[key] for key in columns])
text = stream.getvalue()
for relative in ['sdk/go/testdata/bond_risk_oracle.tsv', 'crates/itofin-py/tests/fixtures/bond_risk_oracle.tsv']:
    path = R / relative
    path.write_text(text)
    print(relative, len(rows), hashlib.sha256(path.read_bytes()).hexdigest())
