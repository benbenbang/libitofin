"""Observe the compiled QuantLib QuantoTermStructure, including mixed clocks."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess

import QuantLib as ql

PIN = "9863b578af0caa4cecabf697196533e84a8308b6"


def curves(mixed):
    reference = ql.Date(15, ql.June, 2026)
    offsets = [0, 30, -20, 15, -10] if mixed else [0] * 5
    counters = [ql.Actual360(), ql.Actual365Fixed(), ql.Actual360(),
                ql.Actual365Fixed(), ql.Actual360()] if mixed else [ql.Actual360()] * 5
    yields = []
    for offset, dc, rates in zip(offsets[:3], counters[:3],
                                 [[0.01, 0.018, 0.025], [0.04, 0.035, 0.05],
                                  [-0.005, 0.01, 0.02]]):
        dates = [reference + offset + n for n in [0, 360, 1080]]
        yields.append(ql.YieldTermStructureHandle(ql.ZeroCurve(dates, rates, dc)))
    asset_reference = reference + offsets[3]
    matrix = ql.Matrix(2, 3)
    for i, row in enumerate([[0.18, 0.22, 0.28], [0.25, 0.29, 0.35]]):
        for j, value in enumerate(row):
            matrix[i][j] = value
    asset = ql.BlackVarianceSurface(asset_reference, ql.NullCalendar(),
                                   [asset_reference + n for n in [180, 360, 720]],
                                   [80.0, 120.0], matrix, counters[3])
    fx_reference = reference + offsets[4]
    fx = ql.BlackVarianceCurve(fx_reference,
                              [fx_reference + n for n in [120, 240, 540]],
                              [0.12, 0.15, 0.19], counters[4])
    return yields, ql.BlackVolTermStructureHandle(asset), ql.BlackVolTermStructureHandle(fx)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    revision = subprocess.check_output(
        ["git", "-C", str(args.source_root), "rev-parse", "HEAD"], text=True
    ).strip()
    if revision != PIN or ql.__version__ != "1.43":
        raise RuntimeError("requires pinned source and compiled QuantLib==1.43")
    path = "ql/termstructures/yield/quantotermstructure.hpp"
    rows = []
    for mixed, strike, correlation in [(False, 100.0, 0.4),
                                        (True, 140.0, -0.7), (True, 60.0, 1.0)]:
        yields, asset, fx = curves(mixed)
        curve = ql.QuantoTermStructure(*yields, asset, strike, fx, 1.2, correlation)
        for t in [0.0, 1e-8, 1e-4, 0.1, 0.5, 1.0, 1.6, 2.0]:
            rows.append({"mixed": mixed, "strike": strike, "correlation": correlation,
                         "time": t, "discount": curve.discount(t, True),
                         "zero_rate": curve.zeroRate(t, ql.Continuous,
                                                     ql.NoFrequency, True).rate()})
    payload = {"source_revision": revision,
               "source_sha256": {path: hashlib.sha256((args.source_root / path).read_bytes()).hexdigest()},
               "compiled_reference": "PyPI QuantLib==1.43, distinct from inspected source revision",
               "observed_api": "QuantoTermStructure.discount and zeroRate via compiled SWIG bindings",
               "rows": rows}
    args.output.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
