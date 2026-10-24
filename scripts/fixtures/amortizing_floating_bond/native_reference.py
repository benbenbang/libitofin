"""Optional QuantLib-only fresh native projection, not the original producer."""

import argparse
import hashlib
import json
import math
from pathlib import Path
import sys


def observed(read):
    try:
        value = float(read())
        if not math.isfinite(value):
            raise ValueError("nonfinite native observation")
        return {"ok": True, "value": value, "hex": value.hex()}
    except RuntimeError as error:
        return {"ok": False, "error_type": type(error).__name__, "message": str(error)}


def native(q, row):
    a = row["terms"]
    def date(value):
        return q.Date(value[2], value[1], value[0])
    settings = q.Settings.instance()
    settings.evaluationDate = date(a["evaluation_date"])
    settings.includeReferenceDateEvents = False
    settings.includeTodaysCashFlows = False
    settings.enforcesTodaysHistoricFixings = False
    if a["at_par"]:
        q.IborCoupon.createAtParCoupons()
    else:
        q.IborCoupon.createIndexedCoupons()
    calendar, counter = getattr(q, a["calendar"])(), q.Actual360()
    schedule = q.Schedule([date(value) for value in a["schedule_dates"]], calendar,
                          q.Unadjusted, q.Unadjusted, q.Period(3, q.Months),
                          q.DateGeneration.Forward, False)
    discount = q.FlatForward(date(a["discount_curve_reference"]), a["discount_quote"], counter, q.Continuous)
    forward = q.FlatForward(date(a["forward_curve_reference"]), a["forward_quote"], counter, q.Continuous)
    index = q.IborIndex("AmortizingPortableReference", q.Period(a["index_tenor_months"], q.Months),
                       a["index_fixing_days"], q.USDCurrency(), calendar, q.Preceding, False,
                       counter, q.YieldTermStructureHandle(forward))
    index.clearFixings()
    for fixing in a["fixings"]:
        index.addFixing(date(fixing["date"]), fixing["value"])
    bond = q.AmortizingFloatingRateBond(a["settlement_days"], a["notionals"], schedule, index,
        counter, fixingDays=a["index_fixing_days"], gearings=[1.0], spreads=a["floating_spreads"],
        paymentConvention=q.Following, issueDate=date(a["issue_date"]),
        exCouponPeriod=q.Period(a["ex_coupon_days"], q.Days), exCouponCalendar=calendar,
        exCouponConvention=q.Unadjusted, exCouponEndOfMonth=False)
    bond.setPricingEngine(q.DiscountingBondEngine(q.YieldTermStructureHandle(discount)))
    result = {"case_id": row["case_id"], "phase": row["phase"], "terms": a,
              "maturity_serial": bond.maturityDate().serialNumber(),
              "settlement_serial": bond.settlementDate().serialNumber(),
              "flows": [{"payment_serial": flow.date().serialNumber(),
                         "principal": q.as_coupon(flow) is None,
                         "amount": observed(flow.amount)} for flow in bond.cashflows()],
              "redemptions": [{"payment_serial": flow.date().serialNumber(),
                               "amount": observed(flow.amount)} for flow in bond.redemptions()]}
    for key, read in [("npv", bond.NPV), ("settlement_value", bond.settlementValue),
                      ("clean_price", bond.cleanPrice), ("dirty_price", bond.dirtyPrice),
                      ("accrued", bond.accruedAmount), ("notional", bond.notional)]:
        result[key] = observed(read)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", action="store_true", required=True)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        raise ValueError("fresh output required")
    sys.dont_write_bytecode = True
    import QuantLib as q
    import QuantLib._QuantLib as binary
    if q.__version__ != "1.43":
        raise ValueError("this projection recipe is qualified only for QuantLib 1.43")
    metadata = {"version": q.__version__, "package": q.__file__, "binary": binary.__file__,
                "binary_sha256": hashlib.sha256(Path(binary.__file__).read_bytes()).hexdigest(),
                "input_sha256": hashlib.sha256(args.input.read_bytes()).hexdigest(),
                "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                "same_source_binary_claim": False,
                "qualification": "fresh optional native projection; not the historical V3/V4 producer"}
    with args.output.open("x") as output:
        output.write(json.dumps({"metadata": metadata}, allow_nan=False) + "\n")
        for line in args.input.read_text().splitlines():
            row = json.loads(line)
            if "terms" in row:
                if row["terms"]["type"] != "floating":
                    raise ValueError("floating-only construction terms required")
                output.write(json.dumps(native(q, row), sort_keys=True, allow_nan=False) + "\n")


if __name__ == "__main__":
    main()
