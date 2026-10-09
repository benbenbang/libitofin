from __future__ import annotations

import argparse
import hashlib
import importlib.metadata
import json
from pathlib import Path
import subprocess

import QuantLib as q


SOURCE_PIN = '9863b578af0caa4cecabf697196533e84a8308b6'


def bond(next_notional, ex_coupon, irregular):
    dates = [q.Date(7, 7, 2026), q.Date(7, 7, 2027)]
    if irregular:
        dates.append(q.Date(19, 10, 2027))
    dates.append(q.Date(7, 7, 2028))
    notionals = [1000.0] + [next_notional] * (len(dates) - 2)
    coupons = [q.FixedRateCoupon(end, nominal, 0.05, q.Actual360(), start, end,
                                q.Date(), q.Date(), end - 6 if ex_coupon else q.Date())
               for start, end, nominal in zip(dates, dates[1:], notionals)]
    return q.Bond(0, q.NullCalendar(), dates[0], coupons)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--source-root', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    revision = subprocess.check_output(['git', '-C', str(args.source_root), 'rev-parse', 'HEAD'], text=True).strip()
    assert revision == SOURCE_PIN
    assert importlib.metadata.version('QuantLib') == '1.43' and q.__version__ == '1.43'
    paths = ['ql/pricingengines/bond/bondfunctions.cpp', 'ql/cashflows/cashflows.cpp', 'ql/instruments/bond.cpp']
    q.Settings.instance().evaluationDate = q.Date(1, 7, 2026)
    curve = q.FlatForward(q.Date(1, 7, 2026), 0.03, q.Actual360(), q.Continuous, q.Annual)
    cases = []
    for irregular in [False, True]:
        for next_notional in [1000.0, 400.0]:
            for ex_coupon in [False, True]:
                b = bond(next_notional, ex_coupon, irregular)
                for day in [3, 7, 10]:
                    settlement = q.Date(day, 7, 2027)
                    for comp, label, spread in [(q.Continuous, 'Continuous', 0.0125), (q.Compounded, 'Compounded', -0.005)]:
                        dirty = q.BondFunctions.dirtyPrice(b, curve, spread, comp, q.Semiannual, settlement)
                        clean = q.BondFunctions.cleanPrice(b, curve, spread, comp, q.Semiannual, settlement)
                        clean_root = q.BondFunctions.zSpread(b, q.BondPrice(clean, q.BondPrice.Clean), curve, comp, q.Semiannual, settlement)
                        dirty_root = q.BondFunctions.zSpread(b, q.BondPrice(dirty, q.BondPrice.Dirty), curve, comp, q.Semiannual, settlement)
                        assert abs(clean_root - spread) < 1e-10 and abs(dirty_root - spread) < 1e-10
                        cases.append({'irregular': irregular, 'next_notional': next_notional, 'ex_coupon': ex_coupon,
                                      'settlement_day': day, 'compounding': label, 'frequency': 'Semiannual', 'spread': spread,
                                      'notional': b.notional(settlement), 'accrued': b.accruedAmount(settlement),
                                      'dirty': dirty, 'clean': clean, 'clean_root': clean_root, 'dirty_root': dirty_root})
    expired = bond(400.0, True, True)
    try:
        q.BondFunctions.dirtyPrice(expired, curve, 0.0125, q.Continuous, q.Semiannual, q.Date(7, 7, 2028))
    except RuntimeError as error:
        maturity_error = str(error)
    else:
        raise AssertionError('maturity must be nontradable')
    result = {'provenance': {'source_revision': revision, 'source_sha256': {p: hashlib.sha256((args.source_root / p).read_bytes()).hexdigest() for p in paths},
                             'compiled_package': 'QuantLib', 'compiled_package_version': q.__version__,
                             'compiled_source_revision': 'not exposed by wheel; distinct from inspected source pin'},
              'price_tolerance': 1e-10, 'spread_tolerance': 1e-10, 'cases': cases, 'maturity_error': maturity_error}
    args.output.write_text(json.dumps(result, indent=2, allow_nan=False) + '\n')
    print(json.dumps({'output': str(args.output), 'cases': len(cases), 'maturity_error': maturity_error}, indent=2))


if __name__ == '__main__':
    main()
