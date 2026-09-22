"""Extract QuantLib's 64-point exponentially fitted Gauss-Laguerre rules."""

import argparse
import hashlib
import re
from pathlib import Path

SOURCE = "ql/pricingengines/vanilla/exponentialfittinghestonengine.cpp"
TARGET = Path("crates/libitofin/src/pricingengines/vanilla/heston_fitting_table.rs")


def render(source: Path) -> str:
    raw = source.read_bytes()
    table = raw.decode().split("const double values4[][129] = {", 1)[1].split(
        "\n        };", 1
    )[0]
    rows = [row.strip() for row in re.findall(r"\{([^{}]+)\}", table)]
    if len(rows) != 147 or any(len(row.split(",")) != 129 for row in rows):
        raise ValueError("unexpected QuantLib fitting table dimensions")
    return (
        "//! QuantLib 1.43 exponentially fitted quadrature table.\n"
        "//! Copyright (C) 2020 Klaus Spanderen; QuantLib license in THIRD_PARTY_NOTICES.md.\n"
        f"//! Source: `{SOURCE}`.\n"
        f"//! Source SHA-256: {hashlib.sha256(raw).hexdigest()}.\n"
        "//! Regenerate with `scripts/generate_heston_fitting_table.py`.\n"
        "//! Each row preserves upstream decimal tokens: frequency, 64 nodes, 64 weights.\n\n"
        "#[rustfmt::skip]\n"
        "#[allow(clippy::excessive_precision, reason = \"Preserve exact upstream decimal tokens\")]\n"
        "pub(super) static FITTING_TABLE: [[f64; 129]; 147] = [\n"
        + "".join(f"    [{row}],\n" for row in rows)
        + "];\n"
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("quantlib", type=Path, help="QuantLib source checkout")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    expected = render(args.quantlib / SOURCE)
    if args.check:
        if TARGET.read_text() != expected:
            raise SystemExit("Heston fitting table differs from its QuantLib source")
    else:
        TARGET.write_text(expected)


if __name__ == "__main__":
    main()
