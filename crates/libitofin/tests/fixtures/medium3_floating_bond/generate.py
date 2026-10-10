"""Independent QuantLib 1.43 native fixtures, not the pinned original revision."""

from pathlib import Path
import QuantLib as ql

ROOT = Path(__file__).resolve().parent
ql.Settings.instance().includeReferenceDateEvents = False
calendar = ql.TARGET()
dates = [
    ql.Date(1, 2, 2026),
    ql.Date(15, 5, 2026),
    ql.Date(1, 11, 2026),
    ql.Date(28, 2, 2027),
]
schedule = ql.Schedule(
    dates,
    calendar,
    ql.Unadjusted,
    ql.Unadjusted,
    ql.Period(6, ql.Months),
    ql.DateGeneration.Backward,
    False,
    [False, True, False],
)
cases = [
    ("plain_par", True, [], [], [], [], False, None, False),
    ("long", True, [], [], [], [], False, None, False),
    ("plain_indexed", False, [], [], [], [], False, None, False),
    ("collar", True, [1.2], [0.004], [0.04], [0.025], False, None, False),
    (
        "sentinel",
        True,
        [0, 1.2, 0],
        [0.1, 0.004, 0.001],
        [ql.nullDouble(), 0.04, ql.nullDouble()],
        [ql.nullDouble(), 0.025, ql.nullDouble()],
        False,
        None,
        False,
    ),
    ("negative", True, [-1.5], [0.08], [0.04], [0.015], False, None, False),
    (
        "mixed",
        True,
        [0, 1.2, 0],
        [0.1, 0.004, 0.001],
        [0.045],
        [0.02],
        False,
        None,
        False,
    ),
    ("fixed", True, [0], [0.01], [0.008], [0.02], False, None, False),
    ("ex_known", True, [1], [0.004], [0.04], [0.02], True, None, False),
    ("holiday", True, [], [], [], [], False, 0, True),
]
lines = ["# QuantLib " + ql.__version__]
for name, at_par, gearings, spreads, caps, floors, known, lag, following in cases:
    ql.IborCoupon.createAtParCoupons() if at_par else ql.IborCoupon.createIndexedCoupons()
    today = ql.Date(12, 5, 2026) if known else ql.Date(5, 1, 2026)
    ql.Settings.instance().evaluationDate = today
    forecast = ql.YieldTermStructureHandle(ql.FlatForward(today, 0.03, ql.Actual360()))
    discount = ql.YieldTermStructureHandle(ql.FlatForward(today, 0.025, ql.Actual360()))
    index = ql.Euribor6M(forecast)
    index.clearFixings()
    if known:
        index.addFixing(ql.Date(29, 1, 2026), 0.035)
    selected_schedule = schedule
    if name == "long":
        selected_schedule = ql.Schedule(
            [
                ql.Date(1, 2, 2026),
                ql.Date(15, 10, 2026),
                ql.Date(15, 4, 2027),
                ql.Date(30, 1, 2028),
            ],
            calendar,
            ql.Unadjusted,
            ql.Unadjusted,
            ql.Period(6, ql.Months),
            ql.DateGeneration.Backward,
            False,
            [False, True, False],
        )
    kwargs = {} if lag is None else {"fixingDays": lag}
    bond = ql.FloatingRateBond(
        2,
        250,
        selected_schedule,
        index,
        ql.Actual360(),
        ql.Following,
        gearings=gearings,
        spreads=spreads,
        caps=caps,
        floors=floors,
        redemption=97,
        issueDate=ql.Date(1, 2, 2026),
        exCouponPeriod=ql.Period(5, ql.Days) if known else ql.Period(),
        exCouponCalendar=calendar,
        exCouponConvention=ql.Preceding,
        fixingConvention=ql.Following if following else ql.Preceding,
        **kwargs,
    )
    ql.setCouponPricer(
        bond.cashflows(),
        ql.BlackIborCouponPricer(
            ql.OptionletVolatilityStructureHandle(
                ql.ConstantOptionletVolatility(
                    today, calendar, ql.Following, 0.2, ql.Actual360()
                )
            )
        ),
    )
    bond.setPricingEngine(ql.DiscountingBondEngine(discount))
    for i, flow in enumerate(bond.cashflows()[:-1]):
        coupon = ql.as_coupon(flow)
        ibor = ql.as_floating_rate_coupon(flow)
        fields = [
            name,
            "coupon",
            i,
            flow.date().serialNumber(),
            coupon.accrualStartDate().serialNumber(),
            coupon.accrualEndDate().serialNumber(),
            coupon.referencePeriodStart().serialNumber(),
            coupon.referencePeriodEnd().serialNumber(),
            coupon.exCouponDate().serialNumber(),
            ibor.fixingDate().serialNumber() if ibor else 0,
            ibor.fixingDays() if ibor else 0,
            ibor.gearing() if ibor else 0,
            ibor.spread() if ibor else 0,
            coupon.rate(),
            flow.amount(),
            coupon.accrualPeriod(),
        ]
        lines.append(
            "\t".join(
                str(x) if isinstance(x, str) else format(x, ".17g") for x in fields
            )
        )
    fields = [
        name,
        "price",
        today.serialNumber(),
        bond.settlementDate().serialNumber(),
        bond.maturityDate().serialNumber(),
        bond.NPV(),
        bond.dirtyPrice(),
        bond.cleanPrice(),
        bond.accruedAmount(),
        bond.notional(),
        bond.redemption().amount(),
    ]
    lines.append(
        "\t".join(str(x) if isinstance(x, str) else format(x, ".17g") for x in fields)
    )
(ROOT / "oracle.tsv").write_text("\n".join(lines) + "\n")
print("\n".join(lines))
