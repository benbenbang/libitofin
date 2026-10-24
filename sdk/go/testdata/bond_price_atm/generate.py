"""Reproduce price-fed ATM fixtures with a separate QuantLib 1.43 installation."""

import argparse
import csv
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import sys

BASE = Path(__file__).resolve().parent


def day(ql, value):
    """Convert an explicit ISO civil date without locale-dependent parsing."""
    year, month, number = map(int, value.split("-"))
    return ql.Date(number, month, year)


def build(ql, case):
    """Construct only independently declared fixed/amortizing/zero oracle cases."""
    calendar, counter = ql.NullCalendar(), ql.Actual360()
    dates = [day(ql, d) for d in case["dates"]]
    ql.Settings.instance().evaluationDate = day(ql, case["reference"])
    curve = ql.FlatForward(
        day(ql, case["reference"]),
        case["discount_rate"],
        ql.Actual365Fixed(),
        ql.Continuous,
        ql.Annual,
    )
    if case["kind"] == "zero":
        bond = ql.ZeroCouponBond(
            case["settlement_days"],
            calendar,
            case["balances"][0],
            dates[-1],
            ql.Unadjusted,
            case["redemption"],
            dates[0],
        )
    else:
        frequency = ql.Annual if case["kind"] == "amortizing" else ql.Semiannual
        schedule = ql.Schedule(
            dates[0],
            dates[-1],
            ql.Period(frequency),
            calendar,
            ql.Unadjusted,
            ql.Unadjusted,
            ql.DateGeneration.Backward,
            False,
        )
        if list(schedule) != dates:
            raise RuntimeError("native schedule differs from declared civil dates")
        options = (
            {
                "exCouponPeriod": ql.Period(case["ex_days"], ql.Days),
                "exCouponCalendar": calendar,
            }
            if case["ex_days"]
            else {}
        )
        if case["kind"] == "amortizing":
            bond = ql.AmortizingFixedRateBond(
                case["settlement_days"],
                case["balances"],
                schedule,
                case["coupon_rates"],
                counter,
                ql.Unadjusted,
                dates[0],
                **options,
            )
        else:
            bond = ql.FixedRateBond(
                case["settlement_days"],
                case["balances"][0],
                schedule,
                case["coupon_rates"],
                counter,
                ql.Unadjusted,
                case["redemption"],
                dates[0],
                **options,
            )
    return bond, curve


def main():
    """Write a fresh directory, never overwrite or rerun an existing output."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cases", type=Path, default=BASE / "cases.jsonl")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        parser.error("output must be a new directory")
    import QuantLib as ql

    if ql.__version__ != "1.43":
        parser.error("a separate QuantLib 1.43 installation is required")
    law_path = BASE / "independent_bond_atm_laws.py"
    module_spec = importlib.util.spec_from_file_location(
        "independent_bond_atm_laws", law_path
    )
    if module_spec is None or module_spec.loader is None:
        raise RuntimeError("independent laws loader unavailable")
    laws = importlib.util.module_from_spec(module_spec)
    module_spec.loader.exec_module(laws)
    records = [json.loads(line) for line in args.cases.read_text().splitlines()]
    if records[0]["record_type"] != "metadata" or any(
        r["record_type"] != "case" for r in records[1:]
    ):
        parser.error("expected one metadata record followed by case records")
    cases = [r["value"] for r in records[1:]]
    results, errors, failures = [], [], []
    for case in cases:
        bond, curve = build(ql, case)
        settlement = day(ql, case["settlement"])
        accrued, notional = bond.accruedAmount(settlement), bond.notional(settlement)
        for clean in case["clean_prices"]:
            initial = laws.solve(case, "clean", clean)
            dirty = float(initial.get("dirty_price_per100", clean))
            for kind, price in [("clean", clean), ("dirty", dirty)]:
                expected = laws.solve(case, kind, price)
                price_type = (
                    ql.BondPrice.Clean if kind == "clean" else ql.BondPrice.Dirty
                )
                try:
                    actual = float(
                        ql.BondFunctions.atmRate(
                            bond, curve, settlement, ql.BondPrice(price, price_type)
                        )
                    )
                except Exception as error:
                    errors.append(
                        [
                            case["id"],
                            case["kind"],
                            kind,
                            price,
                            expected.get("domain", "unexpected_error"),
                            type(error).__name__,
                            str(error),
                        ]
                    )
                    domain = expected.get("domain", "")
                    phrase = (
                        "non tradable"
                        if domain == "nontradable_current_notional_zero"
                        else "null bps"
                    )
                    if (
                        expected["status"] != "error"
                        or not isinstance(error, RuntimeError)
                        or phrase not in str(error)
                    ):
                        failures.append(case["id"])
                    continue
                if expected["status"] != "ok":
                    failures.append(case["id"])
                    continue
                if (
                    not all(math.isfinite(v) for v in [actual, accrued, notional])
                    or abs(actual - float(expected["atm"])) > 2e-13
                    or abs(accrued - float(expected["accrued_per100_current"])) > 2e-12
                    or notional != float(expected["current_notional"])
                ):
                    failures.append(case["id"])
                results.append(
                    [
                        case["id"],
                        case["kind"],
                        case["balances"][0],
                        case["dates"][0],
                        case["dates"][-1],
                        case["reference"],
                        case["settlement"],
                        case["ex_days"],
                        case["discount_rate"],
                        kind,
                        price,
                        notional,
                        accrued,
                        actual,
                        expected["atm"],
                    ]
                )
    args.output.mkdir(parents=True)
    success_header = [
        "id",
        "kind",
        "face",
        "start",
        "end",
        "reference",
        "settlement",
        "ex_days",
        "discount_rate",
        "price_kind",
        "price",
        "current_notional",
        "accrued_per100",
        "native_atm",
        "independent_atm_100digits",
    ]
    error_header = [
        "id",
        "kind",
        "price_kind",
        "price",
        "independent_domain",
        "native_error_type",
        "native_error",
    ]
    for name, header, rows in [
        ("bond_atm_from_price_reference.csv", success_header, results),
        ("bond_atm_from_price_errors.csv", error_header, errors),
    ]:
        with (args.output / name).open("x", newline="") as stream:
            writer = csv.writer(stream, lineterminator="\n")
            writer.writerow(header)
            writer.writerows(rows)
    native_path = Path(sys.modules["QuantLib._QuantLib"].__file__)
    receipt = {
        "quantlib_version": ql.__version__,
        "native_sha256": hashlib.sha256(native_path.read_bytes()).hexdigest(),
        "cases_sha256": hashlib.sha256(args.cases.read_bytes()).hexdigest(),
        "laws_sha256": hashlib.sha256(law_path.read_bytes()).hexdigest(),
        "cases": len(cases),
        "success_rows": len(results),
        "error_rows": len(errors),
        "failures": failures,
        "qualification": "Portable adaptation of the executed independent producer; not claimed to use its original absolute interpreter/image paths.",
    }
    (args.output / "generation-receipt.json").write_text(
        json.dumps(receipt, indent=2, sort_keys=True) + "\n"
    )
    if failures:
        raise RuntimeError("strict independent failures preserved: " + repr(failures))


if __name__ == "__main__":
    main()
