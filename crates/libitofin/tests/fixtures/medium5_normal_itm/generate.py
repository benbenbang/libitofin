"""Regenerate native QuantLib 1.43 and independent extreme-range references."""

import csv
import sys
from decimal import Decimal, localcontext
from pathlib import Path

import QuantLib as ql


EXPECTED_VERSION = "1.43"


def decimal_cdf(x):
    """Integrate the normal density by its convergent power series at |x| <= 2."""
    with localcontext() as context:
        context.prec = 90
        pi = Decimal(
            "3.141592653589793238462643383279502884197169399375105820974944592307816406286208998628"
        )
        term = x
        total = term
        for n in range(1, 1000):
            term *= -x * x * Decimal(2 * n - 1) / Decimal(2 * n * (2 * n + 1))
            total += term
            if abs(term) < Decimal("1e-85"):
                break
        else:
            raise RuntimeError("normal integral did not converge")
        return Decimal("0.5") + total / (2 * pi).sqrt()


def main():
    if ql.__version__ != EXPECTED_VERSION:
        raise RuntimeError(
            f"expected QuantLib {EXPECTED_VERSION}, got {ql.__version__}"
        )
    cases = [
        ("positive", 0.02, 0.03, 0.01),
        ("negative", -0.005, -0.01, 0.01),
        ("cross_zero", -0.02, 0.03, 0.02),
        ("atm", -0.01, -0.01, 0.02),
        ("negative_tail", 8.0, 0.0, 1.0),
        ("far_tail", 30.0, 0.0, 1.0),
        ("positive_tail", -8.0, 0.0, 1.0),
        ("tiny", 1e-300, 2e-300, 1e-300),
        ("adjacent", 1.0, float.fromhex("0x1.0000000000001p+0"), 2**-52),
        ("large", 1e300, 1.1e300, 1e299),
        ("zero_itm", -2.0, 3.0, 0.0),
        ("zero_otm", 3.0, -2.0, 0.0),
        ("zero_atm", -0.03, -0.03, 0.0),
        ("signed_zero", -0.0, 0.0, -0.0),
        ("subnormal", 0.0, float.fromhex("0x0.0000000000001p-1022"), 1.0),
    ]
    path = Path(__file__).with_name("reference.tsv")
    with path.open("w", encoding="utf-8", newline="") as output:
        writer = csv.writer(output, delimiter="\t", lineterminator="\n")
        writer.writerow(
            ["case", "option", "strike", "forward", "std_dev", "expected", "source"]
        )
        for name, strike, forward, std_dev in cases:
            for label, option in [("Call", ql.Option.Call), ("Put", ql.Option.Put)]:
                value = ql.bachelierBlackFormulaAssetItmProbability(
                    option, strike, forward, std_dev
                )
                writer.writerow(
                    [
                        name,
                        label,
                        repr(strike),
                        repr(forward),
                        repr(std_dev),
                        repr(value),
                        "QL1.43",
                    ]
                )
        maximum = sys.float_info.max
        for label, sign in [("Call", 1), ("Put", -1)]:
            value = float(decimal_cdf(Decimal(2 * sign)))
            writer.writerow(
                [
                    "overflow_scaled",
                    label,
                    repr(-maximum),
                    repr(maximum),
                    repr(maximum),
                    repr(value),
                    "Decimal90",
                ]
            )
    print(f"wrote {path.name}: 30 native and 2 Decimal90 cases")


if __name__ == "__main__":
    main()
