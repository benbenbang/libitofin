"""Produce a qualified macOS native CPI leg oracle from pinned headers and a wheel."""
import argparse
import csv
import ctypes
import hashlib
import json
import math
import os
import subprocess
from pathlib import Path

import QuantLib as q
import QuantLib._QuantLib as native


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--quantlib-source',required=True,type=Path)
    parser.add_argument('--boost-include',required=True,type=Path)
    parser.add_argument('--build-dir',required=True,type=Path)
    parser.add_argument('--output-dir',required=True,type=Path)
    parser.add_argument('--cpp-source',type=Path,default=Path(__file__).parent / 'fixtures/cpi-base-date/base_leg_independent.cpp')
    parser.add_argument('--sdkroot',type=Path)
    args=parser.parse_args()
    if os.uname().sysname!='Darwin': raise RuntimeError('wheel-linked helper supports macOS only')
    args.build_dir.mkdir(parents=True,exist_ok=True);args.output_dir.mkdir(parents=True,exist_ok=True)
    library=args.build_dir/'base_leg_independent.dylib';deps=args.build_dir/'base_leg_independent.d'
    cmd=['clang++','-std=c++17','-dynamiclib','-undefined','dynamic_lookup','-MMD','-MF',str(deps),
         '-I'+str(args.quantlib_source),'-I'+str(args.boost_include),str(args.cpp_source),'-o',str(library)]
    env=os.environ.copy()
    if args.sdkroot: env['SDKROOT']=str(args.sdkroot)
    done=subprocess.run(cmd,env=env,text=True,capture_output=True)
    (args.build_dir/'compile.log').write_text(done.stdout+done.stderr);done.check_returncode()
    ctypes.CDLL(native.__file__,mode=os.RTLD_GLOBAL)
    harness=ctypes.CDLL(str(library.resolve()),mode=os.RTLD_GLOBAL)
    harness.run_oracle.argtypes=[ctypes.c_char_p,ctypes.c_char_p];harness.run_oracle.restype=ctypes.c_int
    flows=args.output_dir/'base-leg-flows-native.tsv';npvs=args.output_dir/'base-leg-npv-native.tsv'
    status=harness.run_oracle(os.fsencode(flows.resolve()),os.fsencode(npvs.resolve()))
    if status: raise RuntimeError('native harness status '+str(status))
    rows=list(csv.DictReader(flows.open(),delimiter='\t'))
    npvrows=list(csv.DictReader(npvs.open(),delimiter='\t'))
    first=[r for r in rows if r['case']=='0' and r['stage']=='0']
    firstnpv=[r for r in npvrows if r['case']=='0' and r['stage']=='0' and r['include']=='0'][-1]
    sanity={'coupon_amount':float(first[0]['amount']),'terminal_amount':float(first[-1]['amount']),
            'npv':float(firstnpv['npv'])}
    expected={'coupon_amount':-51.41023477226254,'terminal_amount':6714.046015132657,'npv':6896.228058286242}
    for key,value in expected.items():
        if not math.isclose(sanity[key],value,rel_tol=2e-14,abs_tol=2e-11): raise AssertionError((key,sanity[key],value))
    if q.Settings.instance().evaluationDate.ISO()!='2025-08-18': raise AssertionError('Settings singleton mismatch')
    headers={}
    for token in deps.read_text().replace('\\\n',' ').split():
        path=Path(token)
        if path.is_file() and path.is_relative_to(args.quantlib_source): headers[str(path.relative_to(args.quantlib_source))]=sha(path)
    meta=dict(oracle_kind='mixed_pinned_headers_and_native_wheel',source_is_wheel_build_provenance=False,
              header_source_revision=subprocess.check_output(['git','-C',str(args.quantlib_source),'rev-parse','HEAD'],text=True).strip(),
              header_hashes=headers,wheel_version=q.__version__,wheel_native_sha256=sha(native.__file__),
              source_cpp_sha256=sha(args.cpp_source),source_python_sha256=sha(__file__),
              harness_sha256=sha(library),fixture_sha256={flows.name:sha(flows),npvs.name:sha(npvs)},
              flow_rows=len(rows),npv_rows=len(npvrows),cases=32,sanity=sanity,
              compile_command=cmd,sdkroot=str(args.sdkroot) if args.sdkroot else None,
              compiler=subprocess.check_output(['clang++','--version'],text=True).splitlines()[0],
              qualification='Three numerical sanity values and shared Settings agree with SWIG. Full independent SWIG reconstruction is separately recorded. Not a universal ABI proof or full-source build.')
    (args.output_dir/'base-leg-native.provenance.json').write_text(json.dumps(meta,indent=2,sort_keys=True)+'\n')
    print(json.dumps({k:v for k,v in meta.items() if k not in ['header_hashes','compile_command']},sort_keys=True))


if __name__=='__main__': main()
