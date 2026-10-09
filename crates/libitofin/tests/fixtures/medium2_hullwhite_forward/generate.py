"""Direct native forward-process observations and high-precision limiting laws."""

import argparse
import ast
from decimal import Decimal, localcontext
import hashlib
import json
import math
import re
from pathlib import Path
import subprocess

import QuantLib as ql

PIN = "9863b578af0caa4cecabf697196533e84a8308b6"


def exact_adjustment(a, sigma, s, t, maturity):
    with localcontext() as context:
        context.prec = 80
        a, sigma, s, t, maturity = map(Decimal, map(str, (a, sigma, s, t, maturity)))
        if a == 0:
            return float(sigma * sigma * (t - s) * (maturity - (t + s) / 2))
        coefficient = sigma * sigma / (a * a)
        return float(coefficient * (1 - (-a * (t - s)).exp())
                     - coefficient / 2 * ((-a * (maturity - t)).exp()
                                          - (-a * (maturity + t - 2 * s)).exp()))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--verify-rust", action="store_true")
    args = parser.parse_args()
    revision = subprocess.check_output(
        ["git", "-C", str(args.source_root), "rev-parse", "HEAD"], text=True
    ).strip()
    assert revision == PIN, revision
    assert ql.__version__ == "1.43", ql.__version__
    source = args.source_root / "ql/processes/hullwhiteprocess.cpp"
    content = source.read_bytes()
    assert b"- M_T(t0, t0+dt, T_)" in content
    assert b"Real shift = 0.0001;" in content
    reference = ql.Date(9, 10, 2026)
    dc = ql.Actual365Fixed()
    flat = ql.YieldTermStructureHandle(ql.FlatForward(reference, 0.03, dc))
    nonflat = ql.YieldTermStructureHandle(ql.ZeroCurve(
        [reference, reference + 365, reference + 1095, reference + 3650],
        [0.01, 0.025, 0.04, 0.035], dc))
    rows = []
    for curved, a, sigma, maturity, t, x, dt in [
        (False, .1, .01, 5., 1., .02, .7),
        (False, .7, .15, 3., .2, -.05, 1.3),
        (False, .05, .0, 0., 1., -.02, .2),
        (False, .1, .02, .5, 1., .04, .8),
        (True, .1, .01, 8., 2., .025, .4),
        (True, .25, .03, 4., 0., .01, .5),
        (True, .3, .02, 10., 4., -.01, .3),
    ]:
        process = ql.HullWhiteForwardProcess(nonflat if curved else flat, a, sigma)
        process.setForwardMeasureTime(maturity)
        rows.append(dict(curved=curved, a=a, sigma=sigma, maturity=maturity,
                         t=t, x=x, dt=dt, x0=process.x0(), alpha=process.alpha(t),
                         b=process.B(t, maturity), m=process.M_T(t, t+dt, maturity),
                         drift=process.drift(t, x), expectation=process.expectation(t, x, dt),
                         variance=process.variance(t, x, dt), std=process.stdDeviation(t, x, dt),
                         evolve=process.evolve(t, x, dt, -.6)))
    limits = []
    for a in [0., 1e-16, 1e-14, 1e-10, 1e-7]:
        process = ql.HullWhiteForwardProcess(flat, a, .02)
        process.setForwardMeasureTime(5.)
        limits.append(dict(a=a, sigma=.02, s=1., t=2., maturity=5.,
                           decimal_m=exact_adjustment(a, .02, 1., 2., 5.),
                           native_m=process.M_T(1., 2., 5.), native_drift=(process.drift(1., .03) if a else None),
                           native_zero_a_drift_nonfinite=(not math.isfinite(process.drift(1., .03)) if not a else False)))
    if args.verify_rust:
        test = Path(__file__).resolve().parents[2] / "medium2_hullwhite_forward.rs"
        rust = test.read_text()
        for constant, expected in [("NATIVE", [(row["curved"],
                [row[key] for key in ["a", "sigma", "maturity", "t", "x", "dt"]],
                [row[key] for key in ["x0", "alpha", "b", "m", "drift", "expectation", "variance", "std", "evolve"]])
                for row in rows]), ("LIMITS", [(row["a"], row["decimal_m"]) for row in limits])]:
            match = re.search(r"const " + constant + r":.*?= (\[.*?\]);", rust, re.DOTALL)
            assert match is not None, constant
            actual = ast.literal_eval(match.group(1).replace("true", "True").replace("false", "False"))
            assert actual == expected, constant
    with localcontext() as context:
        context.prec = 80
        scaled_b = float((1 - Decimal(-1).exp()))
    result = dict(scaled_b_at_a_1e_minus_16_duration_1e16=scaled_b, source_revision=PIN, source_sha256=hashlib.sha256(content).hexdigest(),
                  compiled_wheel="QuantLib 1.43", ordinary=rows, limiting=limits)
    args.output.write_text(json.dumps(result, indent=2, sort_keys=True, allow_nan=False) + "\n")


if __name__ == "__main__":
    main()
