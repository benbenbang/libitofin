import argparse
import ctypes
import datetime
import hashlib
import json
import math
import os
from pathlib import Path
import stat
import sys
import traceback

Q = Path('/Users/bn/Git/benbenbang/ito-fin/local/codex/binding-wave7-20261024/qa-math')
PINS = {'independent_payoff_laws.py': '1f6d862f7c2193286e0aba73e5c566d1c69f1a09ed638717ed81514c4ab8a262', 'fd-cases-v1.json': '178a30f8183447bca1f695096e53d9aafd8756a8c27521e071ed829a96d533ce', 'oracle-metadata-v1.json': 'b59188edbc218455e73c5ce650e038b059834cb81f85786a162a9a6dd89f78e2'}


def require(value, message):
    if not value:
        raise RuntimeError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def ordinary_file(path):
    require(path.is_absolute() and '..' not in path.parts, 'Absolute ordinary path required')
    for part in path.parts:
        require(part != 'env.sh' and not part.startswith('.env'), 'Secret/environment path excluded')
    require(path.resolve() == path and stat.S_ISREG(path.lstat().st_mode), 'Ordinary physical file required')
    data = path.read_bytes()
    st = path.stat()
    return data, {'bytes': len(data), 'mode': stat.S_IMODE(st.st_mode), 'dev': st.st_dev, 'ino': st.st_ino, 'mtime_ns': st.st_mtime_ns, 'ctime_ns': st.st_ctime_ns, 'sha256': sha(data)}


def inputs():
    rows = {}
    for name, expected in PINS.items():
        data, rows[name] = ordinary_file(Q / name)
        require(sha(data) == expected, 'Prepared input changed: ' + name)
    meta = json.loads((Q / 'oracle-metadata-v1.json').read_bytes())
    for row in meta['files']:
        data, observed = ordinary_file(Path(row['path']))
        require(sha(data) == row['sha256'] and observed['bytes'] == row['bytes'] and observed['mode'] == int(row['physical_mode'], 8), 'Oracle/source metadata identity differs')
        rows[row['path']] = observed
    return rows


def capture(call):
    try:
        value = float(call())
        return {'status': 'ok', 'value': value, 'hex': value.hex()} if math.isfinite(value) else {'status': 'nonfinite', 'repr': repr(value)}
    except Exception as exc:
        return {'status': 'error', 'type': type(exc).__name__, 'message': str(exc)}


def save(name, value):
    with (out / name).open('x') as stream:
        json.dump(value, stream, indent=2, sort_keys=True, allow_nan=False)
        stream.write('\n')
        stream.flush()
        os.fsync(stream.fileno())


def payoff(case):
    side = ql.Option.Call if case['side'] == 'call' else ql.Option.Put
    return ql.AssetOrNothingPayoff(side, case['strike']) if case['family'] == 'asset' else ql.GapPayoff(side, case['strike'], case['second_strike'])


def market(case):
    dc = ql.Actual360()
    risk = ql.YieldTermStructureHandle(ql.FlatForward(today, case['r'], dc))
    div = ql.YieldTermStructureHandle(ql.FlatForward(today, case['q'], dc))
    vol = ql.BlackVolTermStructureHandle(ql.BlackConstantVol(today, ql.NullCalendar(), case['vol'], dc))
    quote = ql.QuoteHandle(ql.SimpleQuote(case['spot']))
    process = ql.BlackScholesMertonProcess(quote, div, risk, vol)
    option = ql.VanillaOption(payoff(case), ql.EuropeanExercise(today + case['days']))
    return process, option


def run_case(raw):
    case = laws['resolve_second_strike'](raw)
    expected = laws['truncated_lognormal'](case)
    row = {'case': case, 'independent_closed_form': expected, 'fd': []}
    try:
        process, option = market(case)
    except Exception as exc:
        row['construction_error'] = {'type': type(exc).__name__, 'message': str(exc)}
        return row
    for damping in plan['damping_by_family'][case['family']]:
        for grid in plan['grids']:
            observation = {'t_grid': grid, 'x_grid': grid, 'damping_steps': damping, 'scheme': 'Douglas'}
            try:
                engine = ql.FdBlackScholesVanillaEngine(process, grid, grid, damping, ql.FdmSchemeDesc.Douglas())
                option.setPricingEngine(engine)
                observation.update({name: capture(getattr(option, method)) for name, method in [('npv', 'NPV'), ('delta', 'delta'), ('gamma', 'gamma'), ('theta', 'theta')]})
            except Exception as exc:
                observation['engine_error'] = {'type': type(exc).__name__, 'message': str(exc)}
            if observation.get('npv', {}).get('status') == 'ok' and expected['status'] == 'ok':
                observation['npv_minus_closed_form'] = observation['npv']['value'] - float(expected['npv'])
            row['fd'].append(observation)
    try:
        option.setPricingEngine(ql.AnalyticEuropeanEngine(process))
        row['native_analytic_independent_crosscheck'] = capture(option.NPV)
    except Exception as exc:
        row['native_analytic_independent_crosscheck'] = {'status': 'error', 'type': type(exc).__name__, 'message': str(exc)}
    return row


def direct_cells():
    rows = []
    for spec in laws['log_cell_cases']():
        loc = math.log(100)
        mesh = ql.FdmMesherComposite(ql.Uniform1dMesher(loc - .2, loc + .2, 3))
        iterator = ql.FdmLinearOpIterator([3], [1], 1)
        center = mesh.location(iterator, 0)
        a = center - mesh.dminus(iterator, 0) / 2
        b = center + mesh.dplus(iterator, 0) / 2
        h = float(spec.get('second_strike', (math.exp(a) + math.exp(b)) / 2))
        case = dict(spec, second_strike=h)
        p = payoff(case)
        fa, fb = p(math.exp(a)), p(math.exp(b))
        accuracy = (fa + fb) * 5e-5 if fa != 0 or fb != 0 else 1e-4
        calc = ql.FdmLogInnerValue(p, mesh, 0)
        rows.append({'specification': spec, 'actual_binary64': {'log_center': center, 'log_a': a, 'log_b': b, 'second_strike': h, 'endpoint_payoff_sum': fa + fb, 'accuracy': accuracy, 'accuracy_le_machine_epsilon': accuracy <= sys.float_info.epsilon}, 'native_point': capture(lambda: calc.innerValue(iterator, 0)), 'native_cell_average': capture(lambda: calc.avgInnerValue(iterator, 0)), 'independent_true_log_average': ((math.exp(b) - math.exp(a)) / (b - a) - h) * (1 if spec['side'] == 'call' else -1)})
    return rows


parser = argparse.ArgumentParser(description='Existing-wheel independent oracle only; requires separate explicit root QA slot grant before invocation')
parser.add_argument('--apply', action='store_true', required=True)
parser.add_argument('--out', type=Path, required=True)
args = parser.parse_args()
require(sys.flags.isolated and sys.dont_write_bytecode, 'Use isolated Python -I -B with no bytecode')
require(args.out.parent == Q and args.out.name == 'fd-wheel-v1' and not args.out.exists(), 'Exact fresh QA-only output required; no replay after partial run')
out = args.out
before = inputs()
out.mkdir(mode=0o700)
try:
    save('intent.json', {'pid': os.getpid(), 'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'script_sha256': sha(Path(__file__).read_bytes()), 'input_pins': PINS, 'scope': 'Genuine separate compiled QuantLib1.43 wheel, no candidate artifacts, no build, no network, no writes to old oracle/source', 'predeclared_acceptance': 'All ordinary positive-spot/strike/vol nonexpired cases have finite native FD price/delta/gamma/theta; native analytic NPV crosschecks independent closed form at 2e-12*max(1,spot,abs(second_strike)); edge errors retained without parity claims; no FD convergence budget silently imposed or relaxed'})
    laws = {'__name__': 'independent_payoff_laws'}
    exec(compile((Q / 'independent_payoff_laws.py').read_bytes(), str(Q / 'independent_payoff_laws.py'), 'exec'), laws)
    import QuantLib as ql
    require(ql.__version__ == '1.43', 'Actual compiled QuantLib version differs')
    native_path = Path(sys.modules['QuantLib._QuantLib'].__file__).resolve()
    metadata = json.loads((Q / 'oracle-metadata-v1.json').read_bytes())
    expected_native = next(r for r in metadata['files'] if r['path'].endswith('/_QuantLib.abi3.so'))
    require(str(native_path) == expected_native['path'] and sha(native_path.read_bytes()) == expected_native['sha256'], 'Actual imported extension identity differs')
    native = ctypes.CDLL(None)
    native._dyld_image_count.restype = ctypes.c_uint32
    native._dyld_get_image_name.argtypes = [ctypes.c_uint32]
    native._dyld_get_image_name.restype = ctypes.c_char_p
    images = sorted({str(Path(native._dyld_get_image_name(i).decode()).resolve()) for i in range(native._dyld_image_count())})
    require(str(native_path) in images, 'Actual loaded QuantLib image absent')
    save('actual-loader.json', {'version': ql.__version__, 'module_path': str(native_path), 'native_sha256': expected_native['sha256'], 'images': images, 'python_executable': sys.executable, 'python_version': sys.version})
    plan = json.loads((Q / 'fd-cases-v1.json').read_bytes())
    year, month, day = plan['evaluation_date']
    today = ql.Date(day, month, year)
    ql.Settings.instance().evaluationDate = today
    observations = []
    failures = []
    for index, case in enumerate(plan['cases']):
        row = run_case(case)
        save(f'case-{index:02d}.json', row)
        observations.append(row)
        ordinary = all(case[k] > 0 for k in ['spot', 'strike', 'vol', 'days'])
        if ordinary:
            if row.get('construction_error'):
                failures.append({'id': case['id'], 'kind': 'ordinary_construction_error'})
            for fd in row['fd']:
                for query in ['npv', 'delta', 'gamma', 'theta']:
                    if fd.get(query, {}).get('status') != 'ok':
                        failures.append({'id': case['id'], 'grid': fd['x_grid'], 'damping': fd['damping_steps'], 'kind': 'ordinary_nonfinite_or_error', 'query': query})
            analytic = row.get('native_analytic_independent_crosscheck', {})
            budget = 2e-12 * max(1, case['spot'], abs(row['case'].get('second_strike', 0)))
            if analytic.get('status') != 'ok' or abs(analytic['value'] - float(row['independent_closed_form']['npv'])) > budget:
                failures.append({'id': case['id'], 'kind': 'independent_analytic_crosscheck', 'native': analytic, 'budget': budget})
    probes = []
    for family in ['asset', 'gap']:
        for side in ['call', 'put']:
            for spot in [99.99999999, 100, 100.00000001, -1, 0]:
                case = {'family': family, 'side': side, 'strike': 100, 'second_strike': 120 if side == 'call' else 80}
                observed = capture(lambda: payoff(case)(spot))
                expected = float(laws['point_payoff'](family, side, spot, 100, case['second_strike']))
                probes.append({'case': case, 'spot': spot, 'native': observed, 'independent_expected': expected})
                if observed.get('status') != 'ok' or abs(observed['value'] - expected) > 1e-12:
                    failures.append({'kind': 'point_payoff', 'case': case, 'spot': spot})
    save('payoff-points.json', probes)
    save('native-log-cells.json', direct_cells())
    save('observations.json', {'schema_version': 1, 'oracle': 'Actual separately compiled QuantLib1.43 wheel, not same-source local9863 build', 'plan': plan, 'cases': observations, 'failures': failures})
    after = inputs()
    require(before == after, 'Old oracle/source/prepared inputs changed across actual oracle run')
    save('input-barriers.json', {'before': before, 'after': after})
    save('receipt.json', {'status': 'ORACLE_COMPLETED_ZERO_ACCEPTANCE_FAILURES' if not failures else 'ORACLE_COMPLETED_WITH_STRICT_FAILURES', 'pid': os.getpid(), 'case_count': len(observations), 'fd_grid_queries': sum(len(r['fd']) * 4 for r in observations), 'point_probe_count': len(probes), 'failures': failures, 'source_revision': '9863b578af0caa4cecabf697196533e84a8308b6', 'separate_wheel': '1.43', 'no_old_oracle_or_source_mutation': before == after, 'no_candidate_dependency': True})
    require(not failures, 'Strict independent oracle acceptance failed; raw outputs preserved')
    print('ORACLE_COMPLETED_ZERO_ACCEPTANCE_FAILURES')
except BaseException:
    save('STOP.json', {'traceback': traceback.format_exc(), 'no_automatic_retry': True})
    raise
