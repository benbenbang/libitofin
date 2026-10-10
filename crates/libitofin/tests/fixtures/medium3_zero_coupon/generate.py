"""Regenerate checked-in vectors using the compiled PyPI QuantLib 1.43 wheel."""

import csv
from pathlib import Path

import QuantLib as ql

assert ql.__version__ == "1.43", ql.__version__
reference = ql.Date(1, 10, 2026)
calendar = ql.WeekendsOnly()
conventions = [ql.Following, ql.ModifiedFollowing, ql.Preceding, ql.Unadjusted]
compounding = [ql.Continuous, ql.Compounded, ql.Simple]
cases = [
    (2, 0, 1000, 100, 30, 27, -1, 0, 0.04),
    (2, 1, 1000, 98, 30, 27, -1, 1, 0.04),
    (0, 2, 2500, 105, 30, 27, -1, 2, -0.02),
    (0, 3, 1, 100, 30, 27, -1, 0, 0.0),
    (2, 0, 100, 100, 761, 0, -1, 1, 0.05),
    (0, 0, 100, 100, 30, 30, -1, 0, 0.04),
    (0, 0, 100, 100, 30, 32, -1, 0, 0.04),
    (0, 0, 100, 100, 30, 33, -1, 0, 0.04),
    (2, 0, 100, 100, 30, 0, 12, 0, 0.04),
    (2, 0, 100, 100, 30, 0, 40, 0, 0.04),
    (0, 0, 0, 100, 30, 27, -1, 0, 0.04),
    (0, 0, 100, 0, 30, 27, -1, 0, 0.04),
]
output = Path(__file__).with_name("native.csv")
with output.open("w", newline="") as file:
    writer = csv.writer(file, lineterminator="\n")
    writer.writerow(
        [
            "lag",
            "convention",
            "face",
            "redemption",
            "maturity_offset",
            "evaluation_offset",
            "issue_offset",
            "compounding",
            "rate",
            "payment_offset",
            "settlement_offset",
            "notional",
            "npv",
            "settlement_value",
            "clean",
            "dirty",
            "accrued",
            "expired",
        ]
    )
    for case in cases:
        lag, convention, face, redemption, maturity, evaluation, issue, comp, rate = (
            case
        )
        ql.Settings.instance().evaluationDate = reference + evaluation
        ql.Settings.instance().includeReferenceDateEvents = False
        ql.Settings.instance().includeTodaysCashFlows = None
        bond = ql.ZeroCouponBond(
            lag,
            calendar,
            face,
            reference + maturity,
            conventions[convention],
            redemption,
            ql.Date() if issue < 0 else reference + issue,
        )
        curve = ql.FlatForward(
            reference, rate, ql.Actual365Fixed(), compounding[comp], ql.Annual
        )
        bond.setPricingEngine(
            ql.DiscountingBondEngine(ql.YieldTermStructureHandle(curve))
        )
        writer.writerow(
            case
            + (
                bond.cashflows()[0].date() - reference,
                bond.settlementDate() - reference,
                bond.notional(),
                bond.NPV(),
                bond.settlementValue(),
                bond.cleanPrice(),
                bond.dirtyPrice(),
                bond.accruedAmount(),
                int(bond.isExpired()),
            )
        )
print(f"QuantLib {ql.__version__}: wrote {len(cases)} native vectors to {output.name}")
