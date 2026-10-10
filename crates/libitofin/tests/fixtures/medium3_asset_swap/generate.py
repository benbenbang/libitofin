"""Generate frozen native QuantLib 1.43 par AssetSwap fixtures."""

import csv
from pathlib import Path

import QuantLib as ql

OUT = Path(__file__).parent
TODAY = ql.Date(2, 1, 2025)
CALENDAR = ql.TARGET()
ql.IborCoupon.createAtParCoupons()
ql.Settings.instance().evaluationDate = TODAY


def schedule(start, end, tenor):
    return ql.Schedule(start, end, ql.Period(tenor, ql.Months), CALENDAR,
                       ql.ModifiedFollowing, ql.ModifiedFollowing,
                       ql.DateGeneration.Backward, False)


def case(name, pay=True, price=102.5, spread=0.001, forward=0.03,
         discount=0.02, start=None, redemption=100.0, include=True, today_override=None):
    ql.Settings.instance().includeTodaysCashFlows = today_override
    bond_schedule = schedule(ql.Date(2, 1, 2024), ql.Date(2, 1, 2029), 12)
    bond = ql.FixedRateBond(2, 1000.0, bond_schedule, [0.05],
                            ql.Actual365Fixed(), ql.Following, redemption)
    fwd = ql.YieldTermStructureHandle(ql.FlatForward(TODAY, forward, ql.Actual365Fixed()))
    disc = ql.YieldTermStructureHandle(ql.FlatForward(TODAY, discount, ql.Actual365Fixed()))
    index = ql.Euribor6M(fwd)
    index.addFixing(ql.Date(30, 12, 2024), 0.031, True)
    floating = ql.Schedule() if start is None else schedule(start, bond.maturityDate(), 6)
    swap = ql.AssetSwap(pay, bond, price, index, spread, floating, ql.Actual360(), True)
    swap.setPricingEngine(ql.DiscountingSwapEngine(disc, include))
    upfront_date = swap.leg(1)[0].date()
    row = [name, int(pay), price, spread, forward, discount,
           0 if start is None else start.serialNumber(), redemption, int(include),
           len(swap.leg(0)), len(swap.leg(1)),
           bond.accruedAmount(upfront_date), swap.leg(1)[0].amount(),
           swap.NPV(), swap.legNPV(0), swap.legNPV(1),
           swap.legBPS(1), swap.fairSpread(), swap.fairCleanPrice()]
    return row, swap


cases = [case("default"), case("receiver", pay=False),
         case("forward_start", start=ql.Date(2, 4, 2025)),
         case("coupon_cutoff", start=ql.Date(2, 1, 2026)),
         case("nonpar_redemption", redemption=105.0),
         case("negative_spread", spread=-0.002),
         case("forward_bump", forward=0.035),
         case("discount_bump", discount=0.04),
         case("upfront_today_included", start=TODAY),
         case("upfront_today_excluded", start=TODAY, include=False),
         case("at_par", price=100.0, spread=0.0),
         case("coupon_today_override", start=TODAY, today_override=True)]
with (OUT / "prices.csv").open("w", newline="") as output:
    writer = csv.writer(output)
    writer.writerow(["case", "pay", "price", "spread", "forward", "discount",
                     "start", "redemption", "include", "bond_count", "float_count",
                     "accrued", "upfront", "npv", "bond_npv", "float_npv", "bps",
                     "fair_spread", "fair_price"])
    writer.writerows(row for row, _ in cases)
with (OUT / "flows.csv").open("w", newline="") as output:
    writer = csv.writer(output)
    writer.writerow(["case", "leg", "date", "amount"])
    for name in ("default", "coupon_cutoff", "nonpar_redemption"):
        row, swap = next(item for item in cases if item[0][0] == name)
        for j, leg in enumerate([swap.leg(0), swap.leg(1)]):
            writer.writerows([name, j, flow.date().serialNumber(), flow.amount()] for flow in leg)
print(f"Generated {len(cases)} cases with native QuantLib {ql.__version__}")
