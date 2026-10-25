#!/usr/bin/env python3
"""Reproduce scalar payoff TSV rows with an independently installed QuantLib."""

import math
from pathlib import Path

import QuantLib as ql

ROOT = Path(__file__).resolve().parents[1]
KINDS = {"plain_vanilla": 0, "cash_or_nothing": 1, "asset_or_nothing": 2, "gap": 3, "percentage_strike": 4}


def rows():
    for option, flag in [(0, ql.Option.Call), (1, ql.Option.Put)]:
        for strike in [-7.5, 0.0, 70.0, 100.0]:
            constructors = [("plain_vanilla", ql.PlainVanillaPayoff(flag, strike), 0.0),
                            ("asset_or_nothing", ql.AssetOrNothingPayoff(flag, strike), 0.0)]
            for cash in [-8.75, 0.0, 6.5]:
                constructors.append(("cash_or_nothing", ql.CashOrNothingPayoff(flag, strike, cash), cash))
            for second in [strike - 17.0, strike + 11.0]:
                constructors.append(("gap", ql.GapPayoff(flag, strike, second), second))
            for kind, payoff, extra in constructors:
                for price in [-20.0, 0.0, math.nextafter(strike, -math.inf), strike, math.nextafter(strike, math.inf), 140.0]:
                    yield KINDS[kind], option, strike, extra, price, payoff(price)
        for moneyness in [-0.25, 0.0, 0.75, 1.0, 1.25, 2.0]:
            payoff = ql.PercentageStrikePayoff(flag, moneyness)
            for price in [-20.0, 0.0, 70.0, 140.0]:
                yield KINDS["percentage_strike"], option, moneyness, 0.0, price, payoff(price)


def main():
    data = list(rows())
    if len(data) != 384:
        raise RuntimeError("unexpected scalar payoff fixture count")
    lines = ["kind\toption\tstrike\textra\tprice\tvalue"]
    lines.extend("\t".join(str(value) if isinstance(value, int) else repr(value) for value in row) for row in data)
    target = ROOT / "sdk/go/testdata/scalar_payoffs.tsv"
    target.write_text("\n".join(lines) + "\n")
    print(f"QuantLib {ql.__version__}: {len(data)} native rows written to {target}")


if __name__ == "__main__":
    main()
