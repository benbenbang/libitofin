"""Independent QuantLib 1.43 Hull-White callable tree quotes."""

import argparse
import json
from pathlib import Path

import QuantLib as q


def date(day, month, year):
    return q.Date(day, month, year).serialNumber()


def fixed(name, events, **options):
    row = dict(
        name=name,
        kind="fixed",
        reference=date(7, 7, 2026),
        evaluation=date(7, 7, 2026),
        start=date(7, 7, 2026),
        maturity=date(7, 7, 2031),
        issue=date(7, 7, 2026),
        calendar="null",
        settlement_days=2,
        face=1000.0,
        redemption=100.0,
        payment_convention=2,
        coupons=[0.05],
        ex_length=0,
        ex_unit=0,
        ex_eom=False,
        schedule_rule=0,
        rate=0.03,
        a=0.1,
        sigma=0.01,
        steps=100,
        events=events,
        curve_dates=[],
        curve_rates=[],
        currency_tolerance=1e-10,
        price_tolerance=1e-11,
    )
    row.update(options)
    return row


def zero(name, events, **options):
    row = fixed(
        name,
        events,
        kind="zero",
        reference=date(12, 10, 2026),
        evaluation=date(12, 10, 2026),
        start=date(9, 10, 2026),
        issue=date(9, 10, 2026),
        maturity=date(12, 10, 2031),
        calendar="target",
        face=1234567.89,
        redemption=107.5,
        payment_convention=1,
        coupons=[],
        a=0.073,
        sigma=0.014,
        steps=120,
        curve_dates=[
            date(12, 10, 2026),
            date(12, 10, 2027),
            date(12, 10, 2030),
            date(12, 10, 2035),
        ],
        curve_rates=[0.015, 0.031, 0.024, 0.04],
        currency_tolerance=2e-8,
        price_tolerance=2e-12,
    )
    row.update(options)
    return row


def event(day, month, year, amount=100.0, direction=0, quote=0):
    return [amount, quote, direction, date(day, month, year)]


def product(row, events, settings):
    calendar = q.NullCalendar() if row["calendar"] == "null" else q.TARGET()
    dc = q.Actual365Fixed()
    events = q.CallabilitySchedule(
        [
            q.Callability(
                q.BondPrice(
                    amount, q.BondPrice.Clean if quote == 0 else q.BondPrice.Dirty
                ),
                q.Callability.Call if direction == 0 else q.Callability.Put,
                q.Date(serial),
            )
            for amount, quote, direction, serial in events
        ]
    )
    if row["kind"] == "zero":
        return q.CallableZeroCouponBond(
            row["settlement_days"],
            row["face"],
            calendar,
            q.Date(row["maturity"]),
            dc,
            q.Unadjusted
            if row["payment_convention"] == 2
            else (q.Following if row["payment_convention"] == 1 else q.Preceding),
            row["redemption"],
            q.Date(row["issue"]),
            events,
        )
    schedule = q.Schedule(
        q.Date(row["start"]),
        q.Date(row["maturity"]),
        q.Period(q.Semiannual),
        calendar,
        q.Unadjusted,
        q.Unadjusted,
        q.DateGeneration.Zero
        if row["schedule_rule"] == 2
        else q.DateGeneration.Backward,
        False,
    )
    period = q.Period(row["ex_length"], q.Days if row["ex_unit"] == 0 else q.Months)
    return q.CallableFixedRateBond(
        row["settlement_days"],
        row["face"],
        schedule,
        row["coupons"],
        dc,
        q.Unadjusted if row["payment_convention"] == 2 else q.Following,
        row["redemption"],
        q.Date(row["issue"]),
        events,
        period,
        q.NullCalendar(),
        q.Unadjusted,
        row["ex_eom"],
    )


def snapshot(bond, curve):
    settlement = bond.settlementDate()
    return dict(
        npv=bond.NPV(),
        settlement_value=bond.settlementValue(),
        dirty=bond.dirtyPrice(),
        clean=bond.cleanPrice(),
        accrued=bond.accruedAmount(),
        settlement=settlement.serialNumber(),
        redemption_date=bond.cashflows()[-1].date().serialNumber(),
        settlement_discount=curve.discount(settlement),
    )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--raw-output", type=Path)
    args = parser.parse_args()
    if q.__version__ != "1.43":
        raise RuntimeError("QuantLib 1.43 required")
    exercise = event(7, 7, 2028)
    put = event(7, 7, 2029, 101.0, 1)
    rows = [
        fixed("fixed_empty", []),
        fixed("fixed_call_clean", [exercise], update_rate=0.045),
        fixed("fixed_call_dirty", [event(7, 8, 2028, 100, 0, 1)]),
        fixed("fixed_put_clean", [put]),
        fixed("fixed_put_dirty", [event(7, 8, 2028, 101, 1, 1)]),
        fixed("fixed_mixed_unsorted", [put, exercise]),
        fixed("fixed_tie_call_put", [exercise, event(7, 7, 2028, 101, 1, 1)]),
        fixed("fixed_tie_put_call", [event(7, 7, 2028, 101, 1, 1), exercise]),
        fixed("fixed_maturity_call", [event(7, 7, 2031)]),
        fixed("fixed_snap_close_coupon", [event(5, 7, 2028)], ex_length=7),
        fixed(
            "fixed_ex_settlement_clean",
            [event(7, 8, 2028)],
            reference=date(3, 7, 2028),
            evaluation=date(3, 7, 2028),
            ex_length=7,
        ),
        fixed(
            "fixed_ex_settlement_dirty",
            [event(7, 8, 2028, 100, 0, 1)],
            reference=date(3, 7, 2028),
            evaluation=date(3, 7, 2028),
            ex_length=7,
        ),
        fixed("fixed_ex_month_eom", [exercise], ex_length=1, ex_unit=2, ex_eom=True),
        fixed("fixed_lag0", [exercise], settlement_days=0),
        fixed("fixed_lag10", [exercise], settlement_days=10),
        fixed(
            "fixed_distinct_reference_evaluation",
            [exercise],
            evaluation=date(8, 7, 2026),
        ),
        fixed("fixed_native_once", [exercise], schedule_rule=2),
        fixed(
            "fixed_filtered_call_put",
            [event(7, 7, 2026, 120), put],
            corrected_filter=True,
        ),
        fixed(
            "fixed_filtered_put_call",
            [event(7, 7, 2026, 120, 1), exercise],
            corrected_filter=True,
        ),
        fixed("fixed_small_face", [exercise], face=100),
    ]
    call = event(17, 6, 2028, 98.75)
    zput = event(11, 2, 2029, 95.25, 1)
    rows += [
        zero("zero_empty", []),
        zero("zero_call_clean", [call]),
        zero("zero_call_dirty", [event(17, 6, 2028, 98.75, 0, 1)]),
        zero("zero_put", [zput]),
        zero(
            "zero_mixed_unsorted",
            [event(11, 2, 2029, 96, 1, 1), event(17, 6, 2028, 93)],
        ),
        zero(
            "zero_tie_call_put", [event(11, 2, 2029, 90), event(11, 2, 2029, 100, 1, 1)]
        ),
        zero(
            "zero_tie_put_call", [event(11, 2, 2029, 100, 1, 1), event(11, 2, 2029, 90)]
        ),
        zero("zero_maturity_call", [event(12, 10, 2031, 70)]),
        zero(
            "zero_preceding_put", [event(10, 10, 2031, 120, 1, 1)], payment_convention=3
        ),
        zero(
            "zero_settlement_filter",
            [
                event(13, 10, 2026, 120),
                event(14, 10, 2026, 50, 0, 1),
                event(15, 10, 2026, 85),
            ],
        ),
        zero("zero_zero_redemption_put", [zput], redemption=0),
        zero(
            "zero_negative_rates", [call], curve_dates=[], curve_rates=[], rate=-0.012
        ),
        zero("zero_lag0", [call], settlement_days=0),
        zero("zero_lag10", [call], settlement_days=10),
        zero(
            "zero_filtered_call_put",
            [event(12, 10, 2026, 120), zput],
            corrected_filter=True,
        ),
        zero(
            "zero_filtered_put_call",
            [event(12, 10, 2026, 120, 1), call],
            corrected_filter=True,
        ),
    ]
    raw = dict(quantlib_version=q.__version__, cases=[])
    for row in rows:
        settings = q.Settings.instance()
        settings.evaluationDate = q.Date(row["evaluation"])
        settings.includeReferenceDateEvents = False
        settings.includeTodaysCashFlows = None
        dc = q.Actual365Fixed()
        quote = q.SimpleQuote(row["rate"])
        curve = (
            q.ZeroCurve(
                [q.Date(serial) for serial in row["curve_dates"]],
                row["curve_rates"],
                dc,
            )
            if row["curve_dates"]
            else q.FlatForward(q.Date(row["reference"]), q.QuoteHandle(quote), dc)
        )
        model = q.HullWhite(q.YieldTermStructureHandle(curve), row["a"], row["sigma"])
        engine = q.TreeCallableFixedRateBondEngine(model, row["steps"])
        bond = product(row, row["events"], settings)
        bond.setPricingEngine(engine)
        raw["cases"].append(dict(name=row["name"], expected=snapshot(bond, curve)))
        if row.get("corrected_filter"):
            settlement = bond.settlementDate().serialNumber()
            eligible = [event for event in row["events"] if event[3] > settlement]
            bond = product(row, eligible, settings)
            bond.setPricingEngine(engine)
        row["expected"] = snapshot(bond, curve)
        if "update_rate" in row:
            quote.setValue(row["update_rate"])
            row["updated_rate"] = snapshot(bond, curve)
            settings.evaluationDate = q.Date(row["evaluation"] + 1)
            row["updated_evaluation"] = snapshot(bond, curve)
    if args.raw_output:
        args.raw_output.write_text(json.dumps(raw, indent=2, sort_keys=True) + "\n")
    print(
        json.dumps(
            dict(quantlib_version=q.__version__, cases=rows), indent=2, sort_keys=True
        )
    )


if __name__ == "__main__":
    main()
