"""Regenerate native QuantLib 1.43 two-input floating payoff fixtures."""

from pathlib import Path
import math
import struct

import QuantLib as ql


def bits(value: float) -> str:
    return struct.pack(">d", value).hex()


def main() -> None:
    if ql.__version__ != "1.43":
        raise RuntimeError("fixtures require native QuantLib 1.43")
    rows = ["option_type\tprice_bits\tstrike_bits\tvalue_bits"]
    pairs = [
        (80.0, 100.0),
        (100.0, 100.0),
        (120.0, 100.0),
        (-20.0, -10.0),
        (-10.0, -20.0),
        (20.0, -10.0),
        (-20.0, 10.0),
        (math.nextafter(100.0, math.inf), 100.0),
        (math.nextafter(100.0, -math.inf), 100.0),
        (0.0, 0.0),
        (-0.0, 0.0),
        (0.0, -0.0),
        (-0.0, -0.0),
        (5e-324, 0.0),
        (-5e-324, 0.0),
        (1e308, -1e308),
        (-1e308, 1e308),
        (math.inf, 100.0),
        (-math.inf, 100.0),
        (100.0, math.inf),
        (100.0, -math.inf),
        (math.inf, math.inf),
        (-math.inf, -math.inf),
        (math.inf, -math.inf),
        (-math.inf, math.inf),
        (math.nan, 100.0),
        (100.0, math.nan),
        (math.nan, math.nan),
    ]
    for label, option_type in [("Call", ql.Option.Call), ("Put", ql.Option.Put)]:
        payoff = ql.FloatingTypePayoff(option_type)
        if payoff.optionType() != option_type:
            raise RuntimeError("unexpected native option type")
        try:
            payoff(100.0)
        except RuntimeError as error:
            if str(error) != "floating payoff not handled":
                raise
        else:
            raise RuntimeError("native one-input payoff unexpectedly succeeded")
        for price, strike in pairs:
            value = payoff(price, strike)
            rows.append("\t".join([label, bits(price), bits(strike), bits(value)]))
    Path(__file__).with_name("quantlib-1.43.tsv").write_text("\n".join(rows) + "\n")


if __name__ == "__main__":
    main()
