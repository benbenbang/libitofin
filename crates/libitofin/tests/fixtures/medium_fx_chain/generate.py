"""Regenerate independent compiled QuantLib exchange-rate chaining observations."""

import argparse
import hashlib
import json
import math
from pathlib import Path
import subprocess

import QuantLib as ql

SOURCE_PIN = "9863b578af0caa4cecabf697196533e84a8308b6"
CURRENCIES = {
    "EUR": ql.EURCurrency(),
    "USD": ql.USDCurrency(),
    "GBP": ql.GBPCurrency(),
    "JPY": ql.JPYCurrency(),
}


def rate(source, target, value):
    return ql.ExchangeRate(CURRENCIES[source], CURRENCIES[target], value)


def number(value):
    return value if math.isfinite(value) else str(value)


def observation(name, first, second, amounts):
    chained = ql.ExchangeRate.chain(first, second)
    exchanges = []
    for currency, value in amounts:
        item = {"currency": currency, "amount": value}
        try:
            result = chained.exchange(ql.Money(value, CURRENCIES[currency]))
            item.update(value=number(result.value()), target=result.currency().code())
        except RuntimeError:
            item["error"] = True
        exchanges.append(item)
    return {
        "name": name,
        "source": chained.source().code(),
        "target": chained.target().code(),
        "rate": number(chained.rate()),
        "type": chained.type(),
        "exchanges": exchanges,
    }


def generate(source_root):
    revision = subprocess.check_output(
        ["git", "-C", str(source_root), "rev-parse", "HEAD"], text=True
    ).strip()
    if revision != SOURCE_PIN or ql.__version__ != "1.43":
        raise RuntimeError("expected pinned source and compiled QuantLib 1.43")
    files = ["ql/exchangerate.cpp", "ql/exchangerate.hpp"]
    subprocess.run(
        ["git", "-C", str(source_root), "diff", "--quiet", "HEAD", "--", *files],
        check=True,
    )
    hashes = {
        name: hashlib.sha256((source_root / name).read_bytes()).hexdigest()
        for name in files
    }
    inputs = [
        ("common_source", ("EUR", "USD", 1.2), ("EUR", "GBP", 0.8)),
        ("source_target", ("EUR", "USD", 1.2), ("GBP", "EUR", 0.8)),
        ("target_source", ("EUR", "USD", 1.2), ("USD", "GBP", 0.8)),
        ("common_target", ("EUR", "USD", 1.2), ("GBP", "USD", 0.8)),
        ("equal_pair", ("EUR", "USD", 1.2), ("EUR", "USD", 1.3)),
        ("opposite_pair", ("EUR", "USD", 1.2), ("USD", "EUR", 1.3)),
    ]
    cases = []
    amounts = [(c, v) for c in CURRENCIES for v in [100.0, -100.0, 0.0]]
    for name, a, b in inputs:
        first, second = rate(*a), rate(*b)
        cases.append(observation(name, first, second, amounts))
        cases.append(observation(name + "_reversed", second, first, amounts))
    eur_gbp = ql.ExchangeRate.chain(
        rate("EUR", "USD", 1.2), rate("USD", "GBP", 0.8)
    )
    cases.append(observation("nested", eur_gbp, rate("GBP", "JPY", 150.0), amounts))
    cases.append(observation("nested_reversed", rate("GBP", "JPY", 150.0), eur_gbp, amounts))
    for name, a, b, amount in [
        ("intermediate_underflow", 1e-200, 1e200, 1e-200),
        ("intermediate_overflow", 1e200, 1e-200, 1e200),
        ("aggregate_overflow", 1e200, 1e200, 1.0),
        ("aggregate_underflow", 1e-200, 1e-200, 1.0),
    ]:
        cases.append(
            observation(
                name, rate("EUR", "USD", a), rate("USD", "GBP", b), [("EUR", amount)]
            )
        )
    return {
        "source_revision": revision,
        "source_sha256": hashes,
        "compiled_quantlib": ql.__version__,
        "compiled_is_source_build": False,
        "cases": cases,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(
        json.dumps(generate(args.source_root), indent=2, allow_nan=False) + "\n"
    )


if __name__ == "__main__":
    main()
