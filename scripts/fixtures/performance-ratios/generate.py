"""Independent Decimal oracle, no itofin or third-party numerical dependencies."""

import json
from decimal import Decimal, localcontext
from pathlib import Path

CASES = [
    ("monthly_hand", [".02", "-.01", ".03", "-.02"], "0", "12"),
    ("positive_mar", [".02", "-.01", ".03", "-.02"], ".01", "1"),
    ("one_shortfall", ["-.01", ".03"], "0", "1"),
    ("negative_mean", ["-.03", "-.01", ".01"], "0", "252"),
    ("tail", ["-.4", ".01", ".01", ".01"], "0", "252"),
    ("zero_mean", ["-1", "1"], "0", "1"),
]


def generate():
    """Direct definition at 80-digit Decimal precision, rounded once to f64."""
    result = []
    with localcontext() as ctx:
        ctx.prec = 80
        for name, samples, target, frequency in CASES:
            values = [Decimal(s) for s in samples]
            threshold = Decimal(target)
            annual = Decimal(frequency)
            count = Decimal(len(values))
            mean = sum(values) / count
            variance = sum((x - mean) ** 2 for x in values) / (count - 1)
            downside = (
                sum(min(x - threshold, Decimal(0)) ** 2 for x in values) / count
            ).sqrt()
            result.append(
                {
                    "name": name,
                    "returns": [float(x) for x in values],
                    "target": float(threshold),
                    "periods_per_year": float(annual),
                    "downside": float(downside),
                    "sharpe": float(
                        (mean - threshold) / variance.sqrt() * annual.sqrt()
                    ),
                    "sortino": float((mean - threshold) / downside * annual.sqrt()),
                }
            )
    return (
        json.dumps(
            {
                "provenance": "Independent 80-digit Python Decimal definitions; generate.py",
                "cases": result,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    )


if __name__ == "__main__":
    Path(__file__).with_name("oracle.json").write_text(generate())
