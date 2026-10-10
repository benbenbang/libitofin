"""Independent compiled QuantLib 1.43 whole-class fixture, no libitofin imports."""

from pathlib import Path
import QuantLib as ql


def date(serial):
    return ql.Date(serial)


def d(day, month, year=2026):
    return ql.Date(day, month, year).serialNumber()


def curve(rate, reference):
    return ql.YieldTermStructureHandle(
        ql.FlatForward(date(reference), rate, ql.Actual360(), ql.Continuous)
    )


base_dates = [d(8, 1), d(8, 4), d(8, 7), d(8, 10), d(8, 1, 2027)]
quote_dates = [d(8, 2), d(8, 5), d(8, 8), d(8, 11), d(8, 2, 2027), d(8, 5, 2027)]
weekend_dates = [d(17, 1), d(17, 4), d(17, 7), d(17, 10), d(17, 1, 2027)]
reference = d(5, 1)
# name, asymmetric, eval, cutoff, npv, spotSettle, include, quoteN, spread, bf, qf, bd, qd, todayOverride
cases = [
    (
        "matched",
        0,
        reference,
        reference,
        reference,
        reference,
        0,
        1270000,
        0.0017,
        0.031,
        0.043,
        0.027,
        0.036,
        -1,
    ),
    (
        "asymmetric",
        1,
        reference,
        reference,
        reference,
        reference,
        0,
        990000,
        -0.0021,
        0.031,
        0.043,
        0.027,
        0.036,
        -1,
    ),
    (
        "dated_spot",
        1,
        reference,
        reference,
        d(6, 1),
        d(8, 1),
        0,
        990000,
        -0.0021,
        0.031,
        0.043,
        0.027,
        0.036,
        -1,
    ),
    (
        "exclude_cutoff",
        0,
        reference,
        d(8, 4),
        d(6, 1),
        reference,
        0,
        1270000,
        0.0017,
        0.031,
        0.043,
        0.027,
        0.036,
        -1,
    ),
    (
        "include_cutoff",
        0,
        reference,
        d(8, 4),
        d(6, 1),
        reference,
        1,
        1270000,
        0.0017,
        0.031,
        0.043,
        0.027,
        0.036,
        -1,
    ),
    (
        "past_fixings",
        0,
        d(9, 4),
        d(9, 4),
        reference,
        reference,
        0,
        1270000,
        0.0017,
        0.031,
        0.043,
        0.027,
        0.036,
        -1,
    ),
    (
        "today_override",
        0,
        d(8, 4),
        d(8, 4),
        reference,
        reference,
        1,
        1270000,
        0.0017,
        0.031,
        0.043,
        0.027,
        0.036,
        0,
    ),
    (
        "updated",
        0,
        reference,
        reference,
        reference,
        reference,
        0,
        1270000,
        0.0017,
        0.052,
        0.075,
        0.05,
        0.06,
        -1,
    ),
    (
        "weekend",
        2,
        reference,
        reference,
        reference,
        reference,
        0,
        990000,
        -0.0021,
        0.031,
        0.043,
        0.027,
        0.036,
        -1,
    ),
    (
        "expired",
        0,
        d(1, 1, 2028),
        d(1, 1, 2028),
        reference,
        reference,
        0,
        1270000,
        0.0017,
        0.031,
        0.043,
        0.027,
        0.036,
        -1,
    ),
]
header = "name,asymmetric,evaluation,cutoff,npv_date,spot_settlement,include,quote_notional,spread,base_forward,quote_forward,base_discount,quote_discount,today_override,base_pv,quote_pv,quote_bps,npv_base,npv_quote,fair_spread"
rows = [header]
for case in cases:
    (
        name,
        asymmetric,
        evaluation,
        cutoff,
        npv_date,
        spot_settlement,
        include,
        quote_n,
        spread,
        bf,
        qf,
        bd,
        qd,
        override,
    ) = case
    ql.Settings.instance().evaluationDate = date(evaluation)
    ql.Settings.instance().includeTodaysCashFlows = (
        None if override < 0 else bool(override)
    )
    ql.Settings.instance().includeReferenceDateEvents = False
    ql.IborCoupon.createAtParCoupons()
    calendar = ql.NullCalendar()
    bs = ql.Schedule([date(x) for x in base_dates], calendar, ql.Unadjusted)
    quote_schedule_dates = (
        weekend_dates
        if asymmetric == 2
        else (quote_dates if asymmetric else base_dates)
    )
    quote_calendar = ql.WeekendsOnly() if asymmetric == 2 else calendar
    qs = ql.Schedule(
        [date(x) for x in quote_schedule_dates], quote_calendar, ql.Unadjusted
    )
    bi = ql.IborIndex(
        "M3Base",
        ql.Period(3, ql.Months),
        0,
        ql.EURCurrency(),
        calendar,
        ql.Unadjusted,
        False,
        ql.Actual360(),
        curve(bf, reference),
    )
    qi = ql.IborIndex(
        "M3Quote",
        ql.Period(3, ql.Months),
        0,
        ql.USDCurrency(),
        calendar,
        ql.Unadjusted,
        False,
        ql.Actual360(),
        curve(qf, reference),
    )
    bi.clearFixings()
    qi.clearFixings()
    for index, dates, fixing in [
        (bi, base_dates, 0.022),
        (qi, quote_schedule_dates, 0.034),
    ]:
        for serial in dates[:-1]:
            if serial < evaluation:
                index.addFixing(date(serial), fixing)
    swap = ql.ConstNotionalCrossCurrencyBasisSwap(
        1000000,
        ql.EURCurrency(),
        bs,
        bi,
        0,
        1,
        quote_n,
        ql.USDCurrency(),
        qs,
        qi,
        spread,
        1,
    )

    def engine(domestic_base):
        dc, fc = (
            (ql.EURCurrency(), ql.USDCurrency())
            if domestic_base
            else (ql.USDCurrency(), ql.EURCurrency())
        )
        dr, fr = (bd, qd) if domestic_base else (qd, bd)
        fx = 1 / 1.27 if domestic_base else 1.27
        return ql.DiscountingConstNotionalCrossCurrencySwapEngine(
            dc,
            curve(dr, reference),
            fc,
            curve(fr, reference),
            ql.QuoteHandle(ql.SimpleQuote(fx)),
            bool(include),
            date(cutoff),
            date(npv_date),
            date(spot_settlement),
        )

    swap.setPricingEngine(engine(False))
    quote_value = swap.NPV()
    values = [-swap.inCcyLegNPV(0), swap.inCcyLegNPV(1), swap.inCcyLegBPS(1)]
    fair = swap.fairRecSpread() if name != "expired" else 0
    swap.setPricingEngine(engine(True))
    values += [swap.NPV(), quote_value, fair]
    rows.append(
        ",".join(
            str(x) if isinstance(x, str) else format(x, ".17g")
            for x in (*case, *values)
        )
    )
Path(__file__).with_name("native.csv").write_text("\n".join(rows) + "\n")
print(f"QuantLib {ql.__version__}: {len(cases)} whole-class fixtures")
