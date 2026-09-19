"""QuantLib 1.43 ISDA helper bootstrap oracle, independently generated.

Run with QuantLib==1.43. Contracts use CDS dates, 1-day protection settlement,
3-day upfront settlement, Actual360 (last coupon includes its final day),
40% recovery, 1% upfront-contract coupon and a flat Actual365Fixed yield curve.
The oracle.csv uses last-day inclusion; bindings.csv omits it. The today.csv
uses last-day inclusion and zero-day protection and upfront settlement.
QuantLib defaultprobabilityhelpers.cpp:143-148,204-208 fixes the ISDA flags.
"""

import csv
from pathlib import Path

import QuantLib as ql

assert ql.__version__ == "1.43", ql.__version__
ql.Settings.instance().evaluationDate = today = ql.Date(15, 6, 2026)
ql.Settings.instance().includeTodaysCashFlows = False
for include_last_day, lag, filename in (
    (True, 3, "oracle.csv"), (False, 3, "bindings.csv"), (True, 0, "today.csv"),
):
    rows = []
    for kind, values in (("spread", [0.005, 0.01, 0.015]), ("upfront", [0.01, 0.02, 0.04])):
        if lag == 0 and kind == "spread":
            continue
        rate = ql.SimpleQuote(0.03)
        discount = ql.YieldTermStructureHandle(
            ql.FlatForward(today, ql.QuoteHandle(rate), ql.Actual365Fixed())
        )
        quotes = [ql.SimpleQuote(value) for value in values]
        helpers = []
        for years, quote in zip([1, 3, 5], quotes):
            args = [
                ql.Period(years, ql.Years), 0 if lag == 0 else 1, ql.TARGET(), ql.Quarterly,
                ql.Following, ql.DateGeneration.CDS, ql.Actual360(), 0.4, discount,
            ]
            tail = [True, True, ql.Date(), ql.Actual360(include_last_day), True, ql.CreditDefaultSwap.ISDA]
            helper = (
                ql.SpreadCdsHelper(ql.QuoteHandle(quote), *args, *tail)
                if kind == "spread"
                else ql.UpfrontCdsHelper(ql.QuoteHandle(quote), 0.01, *args, lag, *tail)
            )
            helpers.append(helper)
        curve = ql.PiecewiseFlatHazardRate(today, helpers, ql.Actual365Fixed())
        for stage in range(3):
            if stage == 1:
                quotes[1].setValue(values[1] + 0.002)
            if stage == 2:
                rate.setValue(0.04)
            for years, helper in zip([1, 3, 5], helpers):
                pillar = helper.pillarDate()
                survival = curve.survivalProbability(pillar)
                rows.append([
                    kind, stage, years, pillar.serialNumber(),
                    f"{survival:.17g}", f"{helper.impliedQuote():.17g}",
                ])
                assert ql.Settings.instance().includeTodaysCashFlows is False
    with Path(__file__).with_name(filename).open("w", newline="") as stream:
        writer = csv.writer(stream, lineterminator="\n")
        writer.writerow(["kind", "stage", "years", "pillar_serial", "survival", "implied_quote"])
        writer.writerows(rows)
