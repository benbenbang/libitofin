"""QuantLib 1.43 inspectors: C++ COS cumulants and Python analytic CHF.

Cumulants come from the independently generated canonical C++ oracle; regenerate
crates/libitofin/tests/data/heston_engines/oracle.csv first using its README.
The canonical C++ fixture uses vendored headers labeled 1.43-dev and numerical
outputs from the QuantLib-Python wheel runtime 1.43.
The CHF is the normalized Heston characteristic function, independent of the
pricing quadrature. MuT is the logarithm of the discount-factor ratio.
"""

import csv
import json
import math
from pathlib import Path

import QuantLib as ql


def generate():
    assert ql.__version__ == "1.43"
    reference = ql.Date(7, 2, 2017)
    ql.Settings.instance().evaluationDate = reference
    dc = ql.Actual365Fixed()
    risk_free = ql.YieldTermStructureHandle(ql.FlatForward(reference, .15, dc))
    dividend = ql.YieldTermStructureHandle(ql.FlatForward(reference, .075, dc))
    process = ql.HestonProcess(risk_free, dividend, ql.QuoteHandle(ql.SimpleQuote(100)), .1, 4, .25, .4, -.75)
    engine = ql.AnalyticHestonEngine(ql.HestonModel(process))
    root = Path(__file__).resolve().parents[3]
    with (root / "crates/libitofin/tests/data/heston_engines/oracle.csv").open() as source:
        cumulants = [[float(v) for v in row[1:]] for row in csv.reader(source) if row[0] == "c"]
    assert len(cumulants) == 13, "regenerate the complete C++ oracle first"
    characteristic = []
    for t in (0., .01, 1., 10.):
        for u in (-2., 0., .5, 3.):
            value = engine.chF(complex(u, 0), t)
            characteristic.append([t, u, value.real, value.imag])
    fixture = {
        "source": "QuantLib wheel runtime 1.43; COS C++ oracle compiled with 1.43-dev headers; AnalyticHestonEngine.chF via Python",
        "reference": "2017-02-07", "risk_free": .15, "dividend": .075,
        "spot": 100, "v0": .1, "kappa": 4, "theta": .25, "sigma": .4, "rho": -.75,
        "cumulants": cumulants, "characteristic": characteristic,
        "mu": [[t, math.log(dividend.discount(t) / risk_free.discount(t))] for t in (0., .01, 1., 10.)],
    }
    Path(__file__).with_name("cos_heston_inspectors.json").write_text(json.dumps(fixture, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    generate()
