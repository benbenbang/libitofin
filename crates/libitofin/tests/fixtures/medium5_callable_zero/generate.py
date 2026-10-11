"""Regenerate bounded native QuantLib 1.43 callable-zero tree fixtures."""

import csv
from pathlib import Path

import QuantLib as ql


def date(text):
    year, month, day = map(int, text.split("-"))
    return ql.Date(day, month, year)


def main():
    if ql.__version__ != "1.43":
        raise RuntimeError("QuantLib 1.43 required")
    reference = date("2026-10-12")
    calendar = ql.TARGET()
    dc = ql.Actual365Fixed()
    curve = ql.YieldTermStructureHandle(
        ql.ZeroCurve(
            [reference, date("2027-10-12"), date("2030-10-12"), date("2035-10-12")],
            [0.015, 0.031, 0.024, 0.04],
            dc,
        )
    )
    cases = [
        ("plain", "Following", ""),
        ("call_clean", "Following", "2028-06-17,C,Clean,98.75"),
        ("call_dirty", "Following", "2028-06-17,C,Dirty,98.75"),
        ("put", "Following", "2029-02-11,P,Clean,95.25"),
        ("mixed", "Following", "2029-02-11,P,Dirty,96;2028-06-17,C,Clean,93"),
        ("tie_call_put", "Following", "2029-02-11,C,Clean,90;2029-02-11,P,Dirty,100"),
        ("tie_put_call", "Following", "2029-02-11,P,Dirty,100;2029-02-11,C,Clean,90"),
        ("maturity_call", "Following", "2031-10-12,C,Clean,70"),
        ("redemption_put", "Preceding", "2031-10-10,P,Dirty,120"),
        (
            "settlement_filter",
            "Following",
            "2026-10-13,C,Clean,120;2026-10-14,C,Dirty,50;2026-10-15,C,Clean,85",
        ),
        ("zero_redemption_put", "Following", "2029-02-11,P,Clean,95.25"),
        ("negative_rates", "Following", "2028-06-17,C,Clean,98.75"),
    ]
    rows = []
    for name, convention, events in cases:
        ql.Settings.instance().evaluationDate = reference
        ql.Settings.instance().includeReferenceDateEvents = False
        ql.Settings.instance().includeTodaysCashFlows = None
        calls = ql.CallabilitySchedule()
        for event in events.split(";") if events else []:
            when, direction, quote, amount = event.split(",")
            calls.append(
                ql.Callability(
                    ql.BondPrice(float(amount), getattr(ql.BondPrice, quote)),
                    ql.Callability.Call if direction == "C" else ql.Callability.Put,
                    date(when),
                )
            )
        redemption = 0.0 if name == "zero_redemption_put" else 107.5
        handle = curve
        if name == "negative_rates":
            handle = ql.YieldTermStructureHandle(ql.FlatForward(reference, -0.012, dc))
        model = ql.HullWhite(handle, 0.073, 0.014)
        bond = ql.CallableZeroCouponBond(
            2,
            1234567.89,
            calendar,
            date("2031-10-12"),
            dc,
            getattr(ql, convention),
            redemption,
            date("2026-10-09"),
            calls,
        )
        bond.setPricingEngine(ql.TreeCallableFixedRateBondEngine(model, 120))
        rows.append(
            [
                name,
                convention,
                events,
                repr(redemption),
                repr(bond.NPV()),
                repr(bond.settlementValue()),
                repr(bond.dirtyPrice()),
                repr(bond.cleanPrice()),
                bond.settlementDate().ISO(),
                bond.cashflows()[-1].date().ISO(),
            ]
        )
    path = Path(__file__).with_name("reference.tsv")
    with path.open("w", newline="", encoding="utf-8") as output:
        writer = csv.writer(output, delimiter="\t", lineterminator="\n")
        writer.writerow(
            [
                "case",
                "convention",
                "events",
                "redemption",
                "npv",
                "settlement",
                "dirty",
                "clean",
                "settlement_date",
                "payment_date",
            ]
        )
        writer.writerows(rows)


if __name__ == "__main__":
    main()
