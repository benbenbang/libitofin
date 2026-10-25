"""Emit compact cash-digital BlackCalculator fixtures from independent QuantLib 1.43.

The compiled wheel is not claimed to have been built from the pinned C++ source.
Degenerate standard deviations are diagnostic rows rejected by the new factory.
"""
import argparse
import hashlib
import math
from pathlib import Path
import sys

import QuantLib as q
import QuantLib._QuantLib as native


QUERIES = (
    "value", "alpha", "beta", "delta_forward", "delta", "elasticity_forward", "elasticity",
    "gamma_forward", "gamma", "theta", "theta_per_day", "vega", "rho",
    "dividend_rho", "strike_sensitivity", "itm_cash_probability", "itm_asset_probability",
)


def scalar(query):
    try:
        value = query()
    except RuntimeError:
        return "error"
    if math.isnan(value):
        return "nan"
    if math.isinf(value):
        return "+inf" if value > 0 else "-inf"
    return repr(value)


def generate():
    if q.__version__ != "1.43":
        raise RuntimeError("QuantLib 1.43 is required")
    digest = hashlib.sha256(Path(native.__file__).read_bytes()).hexdigest()
    lines = [f"# QuantLib {q.__version__}; native SHA256 {digest}",
             "# Source reference 9863b578af0caa4cecabf697196533e84a8308b6 is not wheel-build provenance.",
             "\t".join(("id", "domain", "option", "strike", "cash_payoff", "forward", "std_dev",
                        "discount", "spot", "maturity", *QUERIES))]
    inputs = [(70., .19), (100., .19), (140., .19), (0., .19),
              (99.999999, 1e-8), (100., 1e-8), (100.000001, 1e-8),
              (70., 0.), (100., 0.), (140., 0.), (100., 1e-20),
              (100., sys.float_info.epsilon), (100., 2 * sys.float_info.epsilon)]
    row_id = 0
    for option, kind in [(0, q.Option.Call), (1, q.Option.Put)]:
        for cash in [-8.75, 0., 6.5]:
            for strike, deviation in inputs:
                forward, discount, spot, maturity = 100., .962, 98., 1.75
                calculator = q.BlackCalculator(
                    q.CashOrNothingPayoff(kind, strike, cash), forward, deviation, discount)
                methods = (
                    calculator.value, calculator.alpha, calculator.beta,
                    calculator.deltaForward, lambda: calculator.delta(spot),
                    calculator.elasticityForward, lambda: calculator.elasticity(spot),
                    calculator.gammaForward, lambda: calculator.gamma(spot),
                    lambda: calculator.theta(spot, maturity), lambda: calculator.thetaPerDay(spot, maturity),
                    lambda: calculator.vega(maturity), lambda: calculator.rho(maturity),
                    lambda: calculator.dividendRho(maturity), calculator.strikeSensitivity,
                    calculator.itmCashProbability, calculator.itmAssetProbability,
                )
                domain = "supported" if deviation > sys.float_info.epsilon else "diagnostic_rejected"
                values = [str(row_id), domain, str(option), *(repr(value) for value in
                          (strike, cash, forward, deviation, discount, spot, maturity)),
                          *(scalar(method) for method in methods)]
                lines.append("\t".join(values))
                row_id += 1
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, nargs="?", default=Path(
        "sdk/go/testdata/cash_digital_black_calculator.tsv"))
    args = parser.parse_args()
    args.output.write_text(generate())
