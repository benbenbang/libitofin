import csv
from pathlib import Path
import QuantLib as q

q.Settings.instance().evaluationDate = q.Date(2, 1, 2026)
curve = q.YieldTermStructureHandle(
    q.FlatForward(q.Date(2, 1, 2026), 0.025, q.Actual365Fixed())
)
schedule = q.Schedule(
    q.Date(15, 1, 2026),
    q.Date(15, 7, 2027),
    q.Period(6, q.Months),
    q.TARGET(),
    q.ModifiedFollowing,
    q.ModifiedFollowing,
    q.DateGeneration.Backward,
    False,
)
caps = [0.03, 0.035, 0.035]
floors = [0.02, 0.015, 0.01]
rows = []
for overnight in [False, True]:
    if overnight:
        leg = q.OvernightLeg(
            [100.0], schedule, q.Sofr(curve), gearings=[1.2], spreads=[0.001]
        )
    else:
        leg = q.IborLeg(
            [100.0], schedule, q.Euribor6M(curve), gearings=[1.2], spreads=[0.001]
        )
    for normal in [False, True]:
        engine = (
            q.BachelierCapFloorEngine(curve, q.QuoteHandle(q.SimpleQuote(0.01)))
            if normal
            else q.BlackCapFloorEngine(
                curve, q.QuoteHandle(q.SimpleQuote(0.2)), q.Actual365Fixed(), 0.02
            )
        )
        for kind in ["cap", "floor", "collar"]:

            def construct(coupons, cap, floor):
                if kind == "cap":
                    return q.Cap(coupons, cap)
                if kind == "floor":
                    return q.Floor(coupons, floor)
                return q.Collar(coupons, cap, floor)

            parent = construct(leg, caps, floors)
            parent.setPricingEngine(engine)
            values = []
            for i in range(len(leg)):
                part = construct([leg[i]], [caps[i]], [floors[i]])
                part.setPricingEngine(engine)
                values.append(part.NPV())
            assert abs(parent.NPV() - sum(values)) < 1e-12
            rows.append(
                {
                    "overnight": overnight,
                    "normal": normal,
                    "kind": kind,
                    "npv": parent.NPV(),
                    "optionlets": values,
                }
            )
with Path(__file__).with_name("quantlib-1.43.csv").open("w", newline="") as output:
    writer = csv.writer(output)
    writer.writerow(
        ["overnight", "normal", "kind", "npv", "optionlet0", "optionlet1", "optionlet2"]
    )
    for row in rows:
        writer.writerow(
            [
                str(row["overnight"]).lower(),
                str(row["normal"]).lower(),
                row["kind"],
                row["npv"],
                *row["optionlets"],
            ]
        )
