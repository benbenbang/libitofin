"""Regenerate selected-input AssetSwap TSV values with the QuantLib 1.43 wheel.

These cases are independently authored, not copied QuantLib source tests.
The wheel and local QuantLib 1.43-dev source revision are distinct references.
Run in a separate environment containing QuantLib==1.43, not the product venv.
"""

import csv
from pathlib import Path

import QuantLib as q

if q.__version__ != "1.43":
    raise RuntimeError("QuantLib 1.43 is required for this fixture")


def date(text):
    """Convert ISO date text into a native oracle date."""
    year, month, day = map(int, text.split("-"))
    return q.Date(day, month, year)


def serial(value):
    """Return a readable cashflow date in the independent oracle output."""
    return value.ISO()


def asset_case(name, kind='fixed', pay=True, price=97.25, spread=.0017, coupon=.043, face=100., redemption=100., forward=.029, discount=.036, include=False, same_day=False):
    evaluation = '2026-01-05' if same_day else '2026-01-02'
    start, maturity = '2026-01-05', '2029-01-05'
    q.Settings.instance().evaluationDate = date(evaluation)
    dc = q.Actual360()
    cal = q.NullCalendar()
    forward_curve = q.YieldTermStructureHandle(q.FlatForward(date(evaluation), forward, dc))
    discount_curve = q.YieldTermStructureHandle(q.FlatForward(date(evaluation), discount, q.Actual365Fixed()))
    index = q.IborIndex('independent-w5-' + name, q.Period(6, q.Months), 0, q.EURCurrency(), cal, q.Unadjusted, False, dc, forward_curve)
    index.addFixing(date(start), forward, True)
    float_schedule = q.Schedule(date(start), date(maturity), q.Period(6, q.Months), cal, q.Unadjusted, q.Unadjusted, q.DateGeneration.Backward, False)
    fixed_schedule = q.Schedule(date(start), date(maturity), q.Period(1, q.Years), cal, q.Unadjusted, q.Unadjusted, q.DateGeneration.Backward, False)
    if kind == 'fixed':
        bond = q.FixedRateBond(0, face, fixed_schedule, [coupon], dc, q.Unadjusted, redemption, date(start))
    else:
        bond = q.ZeroCouponBond(0, cal, face, date(maturity), q.Unadjusted, redemption, date(start))
    engine = q.DiscountingSwapEngine(discount_curve, include)
    def build(p, s, direction=pay):
        result = q.AssetSwap(direction, bond, p, index, s, float_schedule, dc, True)
        result.setPricingEngine(engine)
        return result
    swap = build(price, spread)
    fair_s, fair_p = swap.fairSpread(), swap.fairCleanPrice()
    values = dict(npv=swap.NPV(), bond_leg_npv=swap.legNPV(0), floating_leg_npv=swap.legNPV(1), floating_leg_bps=swap.legBPS(1), fair_spread=fair_s, fair_clean_price=fair_p)
    checks = dict(fair_spread_rebuilt_npv=build(price, fair_s).NPV(), fair_clean_price_rebuilt_npv=build(fair_p, spread).NPV(), spread_bump_npv=build(price, spread + .0001).NPV(), reversed_npv=build(price, spread, not pay).NPV())
    flows = {'bond': [[serial(cf.date()), cf.amount()] for cf in swap.leg(0)], 'floating': [[serial(cf.date()), cf.amount()] for cf in swap.leg(1)]}
    return dict(name=name, kind=kind, pay_bond_coupon=pay, clean_price=price, spread=spread, coupon=coupon, face=face, redemption=redemption, forward=forward, discount=discount, include_settlement_flows=include, evaluation=evaluation, start=start, maturity=maturity, bond_day_count='Actual360', floating_day_count='Actual360', discount_day_count='Actual365Fixed', calendar='NullCalendar', values=values, checks=checks, flows=flows)


rows = [asset_case('fixed-pay'), asset_case('fixed-receive', pay=False, price=104.2, spread=-.0023, coupon=.017), asset_case('zero-pay', kind='zero', price=89.3, spread=0), asset_case('zero-receive-nonpar', kind='zero', pay=False, price=110.1, spread=.0061, redemption=113), asset_case('fixed-large', face=1e6, price=98.4, spread=-.0012), asset_case('fixed-negative', forward=-.012, discount=-.007, coupon=.002), asset_case('upfront-included', same_day=True, include=True), asset_case('upfront-excluded', same_day=True, include=False)]
root = Path(__file__).resolve().parents[1]
keys = ["name", "kind", "pay_bond_coupon", "clean_price", "spread", "coupon", "face", "redemption", "forward", "discount", "include_settlement_flows", "evaluation"]
for directory in ["crates/itofin-py/tests/fixtures", "sdk/go/testdata", "crates/libitofin-ffi/tests"]:
    with (root / directory / "asset_swap_native.tsv").open("w", newline="") as output:
        writer = csv.writer(output, delimiter="\t", lineterminator="\n")
        writer.writerow(keys + list(rows[0]["values"]))
        for row in rows:
            writer.writerow([int(row[key]) if isinstance(row[key], bool) else row[key] for key in keys] + list(row["values"].values()))
