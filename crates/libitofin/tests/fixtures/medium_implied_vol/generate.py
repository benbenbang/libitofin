"""Generate independent compiled QuantLib observations for implied Black variance."""

import argparse
import hashlib
import json
import math
from pathlib import Path
import subprocess

import QuantLib as ql

PIN = "9863b578af0caa4cecabf697196533e84a8308b6"


def observe(original, reset, times, strikes):
    shift = original.dayCounter().yearFraction(original.referenceDate(), reset)
    rows = []
    for t in times:
        for strike in strikes:
            variance = original.blackForwardVariance(shift, shift + t, strike, True)
            maturity = 1e-5 if t == 0.0 else t
            epsilon_variance = original.blackForwardVariance(
                shift, shift + maturity, strike, True
            )
            rows.append({
                "time": t,
                "strike": strike,
                "variance": variance,
                "volatility": math.sqrt(epsilon_variance / maturity),
            })
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    revision = subprocess.check_output(
        ["git", "-C", str(args.source_root), "rev-parse", "HEAD"], text=True
    ).strip()
    if revision != PIN or ql.__version__ != "1.43":
        raise RuntimeError("requires pinned QuantLib sources and compiled QuantLib==1.43")
    paths = [
        "ql/termstructures/volatility/equityfx/impliedvoltermstructure.hpp",
        "ql/termstructures/volatility/equityfx/blackvoltermstructure.hpp",
    ]
    source_hashes = {
        path: hashlib.sha256((args.source_root / path).read_bytes()).hexdigest()
        for path in paths
    }
    reference = ql.Date(15, ql.June, 2026)
    reset = reference + 180
    dc = ql.Actual360()
    dates = [reference + n for n in [180, 360, 720]]
    original = ql.BlackVarianceCurve(reference, dates, [0.18, 0.22, 0.28], dc)
    handle = ql.RelinkableBlackVolTermStructureHandle(original)
    times = [0.0, 1e-6, 0.25, 0.5, 1.0, 1.5, 2.0]
    curve_rows = observe(handle, reset, times, [100.0])
    replacement = ql.BlackVarianceCurve(
        reference + 30,
        [reference + 210, reference + 390, reference + 750],
        [0.21, 0.25, 0.31],
        ql.Actual365Fixed(),
    )
    handle.linkTo(replacement)
    relink_rows = observe(handle, reset, [0.0, 0.25, 0.75], [100.0])
    matrix = ql.Matrix(2, 3)
    for i, row in enumerate([[0.2, 0.24, 0.3], [0.3, 0.34, 0.4]]):
        for j, value in enumerate(row):
            matrix[i][j] = value
    surface = ql.BlackVarianceSurface(reference, ql.TARGET(), dates, [80.0, 120.0], matrix, dc)
    payload = {
        "source_revision": revision,
        "source_sha256": source_hashes,
        "compiled_reference": "PyPI QuantLib==1.43, distinct from the inspected source revision",
        "inputs": {
            "reference": "2026-06-15",
            "reset_days": 180,
            "curve_node_days": [180, 360, 720],
            "curve_vols": [0.18, 0.22, 0.28],
            "day_counter": "Actual/360",
            "replacement_reference_days": 30,
            "replacement_node_days": [210, 390, 750],
            "replacement_vols": [0.21, 0.25, 0.31],
            "replacement_day_counter": "Actual/365 (Fixed)",
            "surface_strikes": [80.0, 120.0],
            "surface_vol_rows": [[0.2, 0.24, 0.3], [0.3, 0.34, 0.4]],
        },
        "curve": curve_rows,
        "relinked": relink_rows,
        "surface": observe(surface, reset, [0.0, 0.25, 0.75], [80.0, 100.0, 120.0]),
        "reference_route": "The wheel lacks ImpliedVolTermStructure. Compiled original forward variance with inspected C++ rebasing and epsilon-adapter laws.",
        "limits": "Strike-dependent case is numerical parity only, not a financial recommendation.",
    }
    args.output.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
