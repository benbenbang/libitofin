"""Regenerate actual native QuantLib 1.43 percentage-strike payoff vectors."""

import math
import struct
from pathlib import Path

import QuantLib as ql


def bits(value):
    return struct.pack(">d", value).hex()


if ql.__version__ != "1.43":
    raise RuntimeError("fixture requires native QuantLib 1.43")

pairs = [
    (0.9, 100.0),
    (1.1, 100.0),
    (0.5, -100.0),
    (1.5, -100.0),
    (-0.5, 80.0),
    (math.nextafter(1.0, 0.0), 1e308),
    (math.nextafter(1.0, math.inf), 1e308),
    (1.0, -0.0),
    (1.0, math.inf),
    (0.5, -0.0),
    (1.5, -0.0),
    (0.5, math.inf),
    (1.5, -math.inf),
    (math.inf, 3.0),
    (-math.inf, -3.0),
    (math.nan, 2.0),
    (0.5, math.nan),
    (-0.0, -0.0),
    (1.0, math.ulp(0.0)),
    (0.0, math.ulp(0.0)),
    (2.0, -math.ulp(0.0)),
    (-1e308, 1e308),
]
rows = ["option_type\tmoneyness_bits\tprice_bits\tvalue_bits"]
for name, option_type in [("Call", ql.Option.Call), ("Put", ql.Option.Put)]:
    for moneyness, price in pairs:
        result = ql.PercentageStrikePayoff(option_type, moneyness)(price)
        rows.append("\t".join([name, bits(moneyness), bits(price), bits(result)]))
Path(__file__).with_name("quantlib-1.43.tsv").write_text("\n".join(rows) + "\n")
