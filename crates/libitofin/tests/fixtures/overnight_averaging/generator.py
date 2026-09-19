"""Generate independent overnight averaging evidence with QuantLib 1.43."""

import csv
import json
from pathlib import Path

import QuantLib as q

assert q.__version__ == "1.43"
MODES = {"simple": q.RateAveraging.Simple, "compound": q.RateAveraging.Compound}
DEFAULTS = {
    "reference_date": "2026-07-01", "evaluation_date": "2026-07-07",
    "start": "2026-07-03", "end": "2026-07-13", "payment": "2026-07-15",
    "nominal": 1000000, "forward_rate": 0.04,
    "coupon_day_counter": "Actual360", "gearing": 1.0, "spread": 0.0,
    "compound_spread": False, "enforce_today": False,
}
HISTORY = [{"date": "2026-07-03", "rate": 0.031},
           {"date": "2026-07-06", "rate": 0.032}]
TODAY = {"date": "2026-07-07", "rate": 0.033}
ALL_FIXINGS = HISTORY + [TODAY] + [
    {"date": "2026-07-08", "rate": 0.034},
    {"date": "2026-07-09", "rate": 0.035},
    {"date": "2026-07-10", "rate": 0.036},
]
ACCRUED_DATES = ["2026-07-03", "2026-07-04", "2026-07-06", "2026-07-08",
                 "2026-07-11", "2026-07-13", "2026-07-15", "2026-07-16"]


def date(value):
    return q.DateParser.parseISO(value)


def reset(evaluation_date, enforce_today=False):
    q.IndexManager.instance().clearHistories()
    q.Settings.instance().evaluationDate = date(evaluation_date)
    q.Settings.instance().enforcesTodaysHistoricFixings = enforce_today


def flat(reference_date, rate):
    return q.YieldTermStructureHandle(q.FlatForward(
        date(reference_date), rate, q.Actual365Fixed()))


def coupon_case(name, overrides, fixings):
    inputs = DEFAULTS | overrides
    results = {}
    for mode, averaging in MODES.items():
        reset(inputs["evaluation_date"], inputs["enforce_today"])
        index = q.Estr(flat(inputs["reference_date"], inputs["forward_rate"]))
        for fixing in fixings:
            index.addFixing(date(fixing["date"]), fixing["rate"])
        dc = q.Actual360() if inputs["coupon_day_counter"] == "Actual360" else q.Actual365Fixed()
        coupon = q.OvernightIndexedCoupon(
            date(inputs["payment"]), inputs["nominal"], date(inputs["start"]),
            date(inputs["end"]), index, gearing=inputs["gearing"],
            spread=inputs["spread"], dayCounter=dc, averagingMethod=averaging,
            compoundSpread=inputs["compound_spread"])
        try:
            results[mode] = {
                "rate": coupon.rate(), "amount": coupon.amount(),
                "accrued_amounts": [coupon.accruedAmount(date(d)) for d in ACCRUED_DATES],
                "effective_spread": coupon.effectiveSpread(),
                "effective_index_fixing": (coupon.effectiveIndexFixing()
                                           if mode == "compound" else ""),
            }
        except RuntimeError as error:
            results[mode] = {"error": str(error)}
    return {"name": name, "inputs": overrides, "fixings": fixings, "results": results}


def ois_case(name, inputs):
    reset(inputs["evaluation_date"])
    results = {}
    for mode, averaging in MODES.items():
        discount = (q.YieldTermStructureHandle() if inputs["discount_rate"] is None
                    else flat(inputs["reference_date"], inputs["discount_rate"]))
        helper = q.OISRateHelper(
            2, q.Period(inputs.get("tenor_years", 1), q.Years), inputs["quote"], q.Estr(), discount,
            paymentLag=inputs["payment_lag"], paymentConvention=q.Following,
            paymentFrequency=(q.Semiannual if inputs.get("payment_frequency") == "semiannual"
                              else q.Annual),
            forwardStart=q.Period(inputs["forward_start_months"], q.Months),
            averagingMethod=averaging, overnightSpread=inputs.get("overnight_spread", 0.0))
        curve = q.PiecewiseLogLinearDiscount(
            date(inputs["reference_date"]), [helper], q.Actual360())
        results[mode] = {
            "maturity": helper.maturityDate().ISO(), "pillar": helper.pillarDate().ISO(),
            "discount": curve.discount(helper.maturityDate()), "quote": helper.impliedQuote(),
        }
    return {"name": name, "inputs": inputs, "results": results}


def swap_case(name, inputs, fixings):
    results = {}
    for mode, averaging in MODES.items():
        reset(inputs["evaluation_date"])
        index = q.Estr(flat(inputs["reference_date"], inputs["forward_rate"]))
        for fixing in fixings:
            index.addFixing(date(fixing["date"]), fixing["rate"])
        swap = q.MakeOIS(
            q.Period(inputs["tenor_years"], q.Years), index, inputs["fixed_rate"],
            effectiveDate=date(inputs["effective_date"]), nominal=inputs["nominal"],
            paymentLag=inputs["payment_lag"], paymentFrequency=q.Annual,
            paymentAdjustmentConvention=q.Following,
            discountingTermStructure=flat(inputs["reference_date"], inputs["discount_rate"]),
            averagingMethod=averaging)
        try:
            results[mode] = {
                "npv": swap.NPV(), "fair_rate": swap.fairRate(),
                "fixed_leg_npv": swap.fixedLegNPV(),
                "overnight_leg_npv": swap.overnightLegNPV(),
                "maturity": swap.maturityDate().ISO(),
            }
        except RuntimeError as error:
            results[mode] = {"error": str(error)}
    return {"name": name, "inputs": inputs, "fixings": fixings, "results": results}


coupon_cases = [
    coupon_case("future", {"evaluation_date": "2026-07-02"}, []),
    coupon_case("historical", {"evaluation_date": "2026-07-14"}, ALL_FIXINGS),
    coupon_case("mixed_today_absent", {}, HISTORY),
    coupon_case("mixed_today_present", {}, HISTORY + [TODAY]),
    coupon_case("today_enforced_absent", {"enforce_today": True}, HISTORY),
    coupon_case("today_enforced_present", {"enforce_today": True}, HISTORY + [TODAY]),
    coupon_case("missing_past", {}, [HISTORY[1]]),
    coupon_case("coupon_actual365", {"coupon_day_counter": "Actual365Fixed"}, HISTORY),
    coupon_case("gearing_spread", {"gearing": 1.7, "spread": 0.002}, HISTORY),
    coupon_case("daily_spread", {"gearing": 1.7, "spread": 0.002, "compound_spread": True}, HISTORY),
    coupon_case("forward_updated", {"forward_rate": 0.06}, HISTORY),
    coupon_case("evaluation_updated", {"evaluation_date": "2026-07-08"}, HISTORY + [TODAY]),
    coupon_case("weekend_start", {"start": "2026-07-04", "evaluation_date": "2026-07-02"}, []),
    coupon_case("weekend_end", {"end": "2026-07-12", "evaluation_date": "2026-07-02"}, []),
    coupon_case("daily_spread_future", {
        "evaluation_date": "2026-07-02", "gearing": 1.7,
        "spread": 0.002, "compound_spread": True,
    }, []),
    coupon_case("daily_spread_historical", {
        "evaluation_date": "2026-07-14", "gearing": 1.7,
        "spread": 0.002, "compound_spread": True,
    }, ALL_FIXINGS),
    coupon_case("today_enforced_partial", {
        "evaluation_date": "2026-07-03", "end": "2026-07-05",
        "payment": "2026-07-06", "enforce_today": True,
    }, []),
    coupon_case("today_enforced_weekend_start", {
        "evaluation_date": "2026-07-03", "start": "2026-07-04", "enforce_today": True,
    }, []),
]
ois_inputs = {
    "evaluation_date": "2026-07-07", "reference_date": "2026-07-07", "quote": 0.05,
    "payment_lag": 2, "forward_start_months": 0, "discount_rate": None,
}
ois_cases = [
    ois_case("baseline", ois_inputs),
    ois_case("quote_updated", ois_inputs | {"quote": 0.06}),
    ois_case("date_updated", ois_inputs | {"quote": 0.06, "evaluation_date": "2026-07-08"}),
    ois_case("forward_start", ois_inputs | {"forward_start_months": 3, "payment_lag": 4}),
    ois_case("external_discount", ois_inputs | {
        "forward_start_months": 3, "discount_rate": 0.03,
        "tenor_years": 3, "payment_frequency": "semiannual",
    }),
]
control = ois_case("endogenous_control", ois_cases[-1]["inputs"] | {"discount_rate": None})
for mode in MODES:
    external = ois_cases[-1]["results"][mode]
    external["endogenous_discount"] = control["results"][mode]["discount"]
    assert abs(external["discount"] - external["endogenous_discount"]) > 1e-12
swap_inputs = {
    "evaluation_date": "2026-07-07", "reference_date": "2026-07-07",
    "effective_date": "2026-07-09", "tenor_years": 1, "nominal": 1000000,
    "fixed_rate": 0.05, "forward_rate": 0.04, "discount_rate": 0.03, "payment_lag": 2,
}
swap_cases = [
    swap_case("future", swap_inputs, []),
    swap_case("forward_updated", swap_inputs | {"forward_rate": 0.06}, []),
    swap_case("historical_missing", swap_inputs | {"effective_date": "2026-07-03"}, []),
    swap_case("mixed", swap_inputs | {"effective_date": "2026-07-03"}, HISTORY),
]
binding_cases = {
    "quantlib": q.__version__,
    "swap_cases": [swap_case("today_start", swap_inputs | {
        "effective_date": swap_inputs["evaluation_date"],
    }, [])],
    "ois_cases": [
        ois_case("spread_initial", ois_inputs | {"overnight_spread": 0.002}),
        ois_case("spread_updated", ois_inputs | {"overnight_spread": 0.003}),
    ],
}
output = Path(__file__).parent
(output / "bindings.json").write_text(json.dumps(binding_cases, indent=2, sort_keys=True) + "\n")
for filename, key, cases in [("ois.json", "ois_cases", ois_cases),
                             ("swaps.json", "swap_cases", swap_cases)]:
    (output / filename).write_text(json.dumps(
        {"quantlib": q.__version__, key: cases}, indent=2, sort_keys=True) + "\n")
columns = ["name", "averaging", *DEFAULTS, "fixings", "rate", "amount", "accrued_amounts", "error",
           "effective_spread", "effective_index_fixing"]
with (output / "coupons.csv").open("w", newline="") as stream:
    writer = csv.DictWriter(stream, fieldnames=columns, lineterminator="\n")
    writer.writeheader()
    for case in coupon_cases:
        for mode, result in case["results"].items():
            inputs = DEFAULTS | case["inputs"]
            inputs = {k: str(v).lower() if isinstance(v, bool) else v for k, v in inputs.items()}
            error = ""
            if "error" in result:
                assert "Missing ESTRON Actual/360 fixing" in result["error"]
                error = "missing_today" if case["name"].startswith("today_enforced") else "missing_past"
            writer.writerow({
                "name": case["name"], "averaging": mode, **inputs,
                "fixings": "|".join(f"{f['date']}:{f['rate']}" for f in case["fixings"]),
                "rate": result.get("rate", ""), "amount": result.get("amount", ""),
                "accrued_amounts": "|".join(
                    f"{d}:{v}" for d, v in zip(ACCRUED_DATES, result.get("accrued_amounts", []))),
                "error": error,
                "effective_spread": result.get("effective_spread", ""),
                "effective_index_fixing": result.get("effective_index_fixing", ""),
            })
