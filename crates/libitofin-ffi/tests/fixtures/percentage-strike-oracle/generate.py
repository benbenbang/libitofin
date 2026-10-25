"""Generate independent constant-volatility QuantLib percentage-strike FD observations.

Run with a Python environment containing QuantLib 1.43. Pass the JSONL output path. Each line is one complete market observation.
The local 1.43-dev source inspected separately is not the wheel's build provenance.
"""

import json
import sys
from pathlib import Path

import QuantLib as ql


def main():
    """Write same-grid prices and Greeks without importing libitofin."""
    today = ql.Date(15, 6, 2026)
    ql.Settings.instance().evaluationDate = today
    dc = ql.Actual360()
    output = Path(sys.argv[1])
    fields = [
        "kind",
        "m",
        "S",
        "r",
        "q",
        "sigma",
        "days",
        "grid",
        "npv",
        "delta",
        "gamma",
        "theta",
    ]
    rows = []
    for spot, rate, dividend, volatility, days in [
        (100, 0.05, 0.02, 0.2, 360),
        (65, -0.01, 0.03, 0.35, 180),
    ]:
        process = ql.BlackScholesMertonProcess(
            ql.QuoteHandle(ql.SimpleQuote(spot)),
            ql.YieldTermStructureHandle(ql.FlatForward(today, dividend, dc)),
            ql.YieldTermStructureHandle(ql.FlatForward(today, rate, dc)),
            ql.BlackVolTermStructureHandle(
                ql.BlackConstantVol(today, ql.NullCalendar(), volatility, dc)
            ),
        )
        for kind, flag in [("call", ql.Option.Call), ("put", ql.Option.Put)]:
            for m in [-0.2, 0, 0.8, 1, 1.2, 2]:
                option = ql.VanillaOption(
                    ql.PercentageStrikePayoff(flag, m),
                    ql.EuropeanExercise(today + days),
                )
                for grid in [100, 400]:
                    option.setPricingEngine(
                        ql.FdBlackScholesVanillaEngine(
                            process, grid, grid, 0, ql.FdmSchemeDesc.Douglas()
                        )
                    )
                    values = [
                        option.NPV(),
                        option.delta(),
                        option.gamma(),
                        option.theta(),
                    ]
                    rows.append(
                        [
                            kind,
                            m,
                            spot,
                            rate,
                            dividend,
                            volatility,
                            days,
                            grid,
                            *values,
                        ]
                    )

    data = [dict(zip(fields, [row[0], *map(float, row[1:])])) for row in rows]
    output.write_text(
        "".join(json.dumps(row, separators=(",", ":")) + "\n" for row in data)
    )


if __name__ == "__main__":
    main()
