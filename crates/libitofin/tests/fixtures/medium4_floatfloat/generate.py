from pathlib import Path

import QuantLib as ql


REFERENCE = ql.Date(2, 1, 2026)
DATES1 = [
    ql.Date(17, m, y)
    for m, y in [(1, 2026), (4, 2026), (7, 2026), (10, 2026), (1, 2027)]
]
DATES2 = [ql.Date(17, m, y) for m, y in [(1, 2026), (7, 2026), (1, 2027)]]


def case(
    name,
    evaluation,
    settlement,
    npv_date,
    include,
    today,
    rates,
    spreads,
    explicit=False,
):
    ql.Settings.instance().evaluationDate = evaluation
    ql.Settings.instance().includeTodaysCashFlows = today
    ql.IndexManager.instance().clearHistories()
    curves = [
        ql.YieldTermStructureHandle(ql.FlatForward(REFERENCE, rate, ql.Actual360()))
        for rate in rates
    ]
    indexes = [
        ql.IborIndex(
            f"M4Float{i}",
            ql.Period(months, ql.Months),
            0,
            ql.EURCurrency(),
            ql.NullCalendar(),
            ql.Unadjusted,
            False,
            ql.Actual360(),
            curves[i],
        )
        for i, months in enumerate([3, 6])
    ]
    schedules = [
        ql.Schedule(dates, ql.WeekendsOnly(), ql.Unadjusted)
        for dates in [DATES1, DATES2]
    ]
    for index, dates in zip(indexes, [DATES1, DATES2]):
        for date in dates[:-1]:
            if date < evaluation:
                index.addFixing(date, 0.021 if index == indexes[0] else 0.031)
    arguments = [
        ql.VanillaSwap.Payer,
        [1_000_000.0] * 4,
        [1_000_000.0] * 2,
        schedules[0],
        indexes[0],
        ql.Actual360(),
        schedules[1],
        indexes[1],
        ql.Actual365Fixed(),
        False,
        False,
        [],
        [spreads[0]] * 4,
        [],
        [],
        [],
        [spreads[1]] * 2,
        [],
        [],
    ]
    if explicit:
        arguments += [ql.Following, ql.Following]
    swap = ql.FloatFloatSwap(*arguments)
    swap.setPricingEngine(
        ql.DiscountingSwapEngine(curves[2], include, settlement, npv_date)
    )
    values = [
        swap.NPV(),
        swap.legNPV(0),
        swap.legNPV(1),
        swap.legBPS(0),
        swap.legBPS(1),
    ]
    values += (
        [swap.fairSpread1(), swap.fairSpread2()]
        if evaluation <= DATES1[-1]
        else [0.0, 0.0]
    )
    inputs = [
        evaluation.serialNumber(),
        settlement.serialNumber(),
        npv_date.serialNumber(),
        int(include),
        -1 if today is None else int(today),
        *rates,
        *spreads,
        int(explicit),
    ]
    return name + "," + ",".join(format(value, ".17g") for value in inputs + values)


rows = [
    "name,evaluation,settlement,npvdate,include,today,forward1,forward2,discount,spread1,spread2,explicit,npv,pv1,pv2,bps1,bps2,fair1,fair2"
]
for name, evaluation, cutoff, include, today, rates, spreads, explicit in [
    (
        "base",
        REFERENCE,
        REFERENCE,
        False,
        None,
        [0.025, 0.034, 0.02],
        [0.001, -0.002],
        False,
    ),
    (
        "negative",
        REFERENCE,
        REFERENCE,
        False,
        None,
        [-0.01, -0.004, -0.002],
        [-0.003, 0.004],
        False,
    ),
    (
        "shifted_npv",
        REFERENCE,
        ql.Date(17, 4, 2026),
        True,
        None,
        [0.028, 0.032, 0.021],
        [0.0, 0.001],
        False,
    ),
    (
        "seasoned",
        ql.Date(18, 4, 2026),
        ql.Date(18, 4, 2026),
        False,
        None,
        [0.025, 0.034, 0.02],
        [0.001, -0.002],
        False,
    ),
    (
        "cutoff_in",
        ql.Date(17, 7, 2026),
        ql.Date(17, 7, 2026),
        True,
        None,
        [0.025, 0.034, 0.02],
        [0.001, -0.002],
        False,
    ),
    (
        "cutoff_out",
        ql.Date(17, 7, 2026),
        ql.Date(17, 7, 2026),
        False,
        None,
        [0.025, 0.034, 0.02],
        [0.001, -0.002],
        False,
    ),
    (
        "today_in",
        ql.Date(17, 7, 2026),
        ql.Date(17, 7, 2026),
        False,
        True,
        [0.025, 0.034, 0.02],
        [0.001, -0.002],
        False,
    ),
    (
        "today_out",
        ql.Date(17, 7, 2026),
        ql.Date(17, 7, 2026),
        True,
        False,
        [0.025, 0.034, 0.02],
        [0.001, -0.002],
        False,
    ),
    (
        "following",
        REFERENCE,
        REFERENCE,
        False,
        None,
        [0.025, 0.034, 0.02],
        [0.001, -0.002],
        True,
    ),
    (
        "expired",
        ql.Date(18, 1, 2027),
        ql.Date(18, 1, 2027),
        False,
        None,
        [0.025, 0.034, 0.02],
        [0.001, -0.002],
        False,
    ),
]:
    npv_date = ql.Date(17, 4, 2026) if name == "shifted_npv" else REFERENCE
    rows.append(
        case(
            name, evaluation, cutoff, npv_date, include, today, rates, spreads, explicit
        )
    )
Path(__file__).with_name("native.csv").write_text("\n".join(rows) + "\n")
