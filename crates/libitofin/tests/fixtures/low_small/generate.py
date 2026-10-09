from __future__ import annotations

import argparse
from decimal import Decimal, localcontext
from fractions import Fraction
import hashlib
import importlib.metadata
import json
from pathlib import Path
import subprocess

import QuantLib as q


def exact_inverse(values):
    n = len(values)
    a = [[Fraction(v) for v in row] + [Fraction(int(i == j)) for j in range(n)] for i, row in enumerate(values)]
    determinant = Fraction(1)
    for k in range(n):
        pivot = next((i for i in range(k, n) if a[i][k]), None)
        if pivot is None:
            return Fraction(0), None
        if pivot != k:
            a[k], a[pivot] = a[pivot], a[k]
            determinant = -determinant
        p = a[k][k]
        determinant *= p
        a[k] = [v / p for v in a[k]]
        for i in range(n):
            if i != k:
                p = a[i][k]
                a[i] = [v - p * w for v, w in zip(a[i], a[k])]
    return determinant, [row[n:] for row in a]


def rows(m):
    return [[m[i][j] for j in range(m.columns())] for i in range(m.rows())]


def product(a, b):
    return [[sum(x * y for x, y in zip(row, column)) for column in zip(*b)] for row in a]


def transpose(a):
    return list(map(list, zip(*a)))


def identity_error(a):
    return max(abs(v - int(i == j)) for i, row in enumerate(a) for j, v in enumerate(row))


def matrix_case(name, values):
    det, inv = exact_inverse(values)
    out = {'name': name, 'input_decimal_strings': values, 'exact_fraction_determinant': str(det), 'exact_fraction_inverse': None if inv is None else [[str(v) for v in row] for row in inv]}
    try:
        source = [[float(v) for v in row] for row in values]
        actual = rows(q.inverse(q.Matrix(source)))
        expected = None if inv is None else [[float(v) for v in row] for row in inv]
        out['compiled_quantlib'] = {'status': 'returned', 'inverse': actual, 'left_identity_max_abs_error': identity_error(product(source, actual)), 'right_identity_max_abs_error': identity_error(product(actual, source))}
        if expected is not None:
            out['compiled_quantlib']['max_component_scaled_error'] = max(abs(v - w) / abs(w) if w else abs(v) for row, erow in zip(actual, expected) for v, w in zip(row, erow))
    except Exception as e:
        out['compiled_quantlib'] = {'status': 'exception', 'type': type(e).__name__, 'message': str(e)}
    return out


def diagonal(value):
    return [[value if i == j else '0' for j in range(3)] for i in range(3)]


def legacy():
    with localcontext() as ctx:
        ctx.prec = 80
        fixing = list(map(Decimal, ['0.5', '1', '2']))
        a, b, c, d = map(Decimal, ['0.2', '0.3', '0.1', '0.15'])
        rho = Decimal('0.25')
        corr = [[(-rho * abs(i - j)).exp() for j in range(3)] for i in range(3)]
        f_corr = [[float(v) for v in row] for row in corr]
        spectral = rows(q.pseudoSqrt(q.Matrix(f_corr), q.SalvagingAlgorithm.Spectral))
        spectral_product = product(spectral, transpose(spectral))
        result = {'reference': '80-digit Decimal.exp implementation of independently inspected QuantLib source laws; no compiled Lm/Lfm class invocation', 'fixing_times': [float(v) for v in fixing], 'parameters_abcd': [float(v) for v in [a, b, c, d]], 'rho': float(rho), 'correlation_decimal80': [[str(v) for v in row] for row in corr], 'correlation': f_corr, 'compiled_quantlib_spectral_root': spectral, 'compiled_quantlib_spectral_root_product': spectral_product, 'spectral_root_product_max_abs_error': max(abs(x - y) for row, erow in zip(spectral_product, f_corr) for x, y in zip(row, erow)), 'times': []}
        for time in ['0', '0.5', '1', '1.5']:
            t = Decimal(time)
            vol = [(a * (f - t) + d) * (-b * (f - t)).exp() + c if f > t else Decimal(0) for f in fixing]
            cov = [[vol[i] * corr[i][j] * vol[j] for j in range(3)] for i in range(3)]
            diffusion = [[float(vol[i]) * v for v in row] for i, row in enumerate(spectral)]
            diffusion_product = product(diffusion, transpose(diffusion))
            fcov = [[float(v) for v in row] for row in cov]
            result['times'].append({'time': float(t), 'volatility_decimal80': [str(v) for v in vol], 'volatility': [float(v) for v in vol], 'covariance_decimal80': [[str(v) for v in row] for row in cov], 'covariance': fcov, 'compiled_quantlib_root_scaled_diffusion': diffusion, 'diffusion_product': diffusion_product, 'diffusion_product_max_abs_error': max(abs(x - y) for row, erow in zip(diffusion_product, fcov) for x, y in zip(row, erow))})
        return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--source-root', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    paths = ['ql/math/matrix.cpp', 'ql/legacy/libormarketmodels/lmlinexpvolmodel.cpp', 'ql/legacy/libormarketmodels/lmexpcorrmodel.cpp', 'ql/legacy/libormarketmodels/lfmcovarproxy.cpp']
    source = {'git_revision': subprocess.check_output(['git', '-C', str(args.source_root), 'rev-parse', 'HEAD'], text=True).strip(), 'sha256': {p: hashlib.sha256((args.source_root / p).read_bytes()).hexdigest() for p in paths}}
    assert source['git_revision'] == '9863b578af0caa4cecabf697196533e84a8308b6'
    assert importlib.metadata.version('QuantLib') == '1.43' and q.__version__ == '1.43'
    cases = [('identity', diagonal('1')), ('pivot_required', [['0', '2', '1'], ['1', '0', '3'], ['4', '1', '8']]), ('dense_integer', [['4', '1', '2'], ['0', '3', '-1'], ['2', '0', '5']]), ('singular', [['1', '2', '3'], ['2', '4', '6'], ['0', '1', '2']]), ('uniform_large', diagonal('1e200')), ('uniform_small', diagonal('1e-200')), ('mixed_diagonal', [['1e-200', '0', '0'], ['0', '1e200', '0'], ['0', '0', '1']]), ('mixed_diagonal_extreme', [['1e-300', '0', '0'], ['0', '1e300', '0'], ['0', '0', '1']]), ('mixed_permutation', [['0', '1e200', '0'], ['1e-200', '0', '0'], ['0', '0', '1']])]
    result = {'provenance': {'source': source, 'compiled': {'package': 'QuantLib', 'package_version': importlib.metadata.version('QuantLib'), 'runtime_version': q.__version__, 'source_revision': 'not exposed by wheel; do not claim identical to source pin'}, 'legacy_compiled_classes_available': {name: hasattr(q, name) for name in ['LmLinearExponentialVolatilityModel', 'LmExponentialCorrelationModel', 'LfmCovarianceProxy']}, 'matrix_reference': 'Exact rational Gaussian elimination implemented independently using Python Fraction; decimal inputs are interpreted as exact rationals', 'root_comparison': 'Compare factor products, not root orientation; roots need not match entrywise'}, 'matrix': [matrix_case(name, values) for name, values in cases], 'legacy': legacy()}
    for case in result['matrix']:
        c = case['compiled_quantlib']
        if case['exact_fraction_inverse'] is not None:
            assert c['status'] == 'returned' and c['left_identity_max_abs_error'] < 1e-13 and c['right_identity_max_abs_error'] < 1e-13
            assert c['max_component_scaled_error'] < 1e-13
    assert result['matrix'][3]['compiled_quantlib']['status'] == 'exception'
    assert result['legacy']['spectral_root_product_max_abs_error'] < 1e-13
    assert all(t['diffusion_product_max_abs_error'] < 1e-13 for t in result['legacy']['times'])
    args.output.write_text(json.dumps(result, indent=2, allow_nan=False) + '\n')
    print(json.dumps({'output': str(args.output), 'matrix_cases': len(cases), 'matrix_outcomes': {v['name']: v['compiled_quantlib']['status'] for v in result['matrix']}, 'legacy_times': len(result['legacy']['times']), 'spectral_product_error': result['legacy']['spectral_root_product_max_abs_error'], 'covariance_product_max_error': max(t['diffusion_product_max_abs_error'] for t in result['legacy']['times'])}, indent=2))


if __name__ == '__main__':
    main()
