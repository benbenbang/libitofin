"""Regenerate whole-instrument OIS side fixtures using native QuantLib 1.43."""

from pathlib import Path

import QuantLib as ql

assert ql.__version__ == "1.43", ql.__version__
TODAY = ql.Date(2, 1, 2026)
ql.Settings.instance().evaluationDate = TODAY
DATES = [TODAY + days for days in (0, 180, 500, 1000, 2000)]
FORWARD = ql.YieldTermStructureHandle(
    ql.DiscountCurve(DATES, [1.0, 0.986, 0.955, 0.905, 0.82], ql.Actual365Fixed())
)
DISCOUNT = ql.YieldTermStructureHandle(
    ql.DiscountCurve(DATES, [1.0, 0.992, 0.978, 0.95, 0.90], ql.Actual365Fixed())
)
INDEX = ql.Estr(FORWARD)
HEADER = "name\tnpv\tfixed_npv\ton_npv\tfixed_bps\ton_bps\tfair_rate\tfair_spread\tfixed_rate\tstart\tend\tpayments"
ROWS = [HEADER]
for name, side, rate, eom in (
    ("payer", ql.OvernightIndexedSwap.Payer, 0.034, False),
    ("receiver", ql.OvernightIndexedSwap.Receiver, 0.034, False),
    ("payer_fair", ql.OvernightIndexedSwap.Payer, None, False),
    ("receiver_fair", ql.OvernightIndexedSwap.Receiver, None, False),
    ("payer_eom", ql.OvernightIndexedSwap.Payer, 0.034, True),
    ("receiver_eom", ql.OvernightIndexedSwap.Receiver, 0.034, True),
):
    swap = ql.MakeOIS(
        ql.Period(30, ql.Months),
        INDEX,
        rate,
        swapType=side,
        effectiveDate=ql.Date(27, 2, 2026) if eom else ql.Date(7, 1, 2026),
        terminationDate=ql.Date(19, 7, 2028),
        nominal=1_230_000.0,
        overnightLegSpread=-0.0012,
        fixedLegDayCount=ql.Actual365Fixed(),
        paymentFrequency=ql.Semiannual,
        paymentLag=2,
        paymentAdjustmentConvention=ql.ModifiedFollowing,
        endOfMonth=eom,
        discountingTermStructure=DISCOUNT,
    )
    values = (
        swap.NPV(),
        swap.fixedLegNPV(),
        swap.overnightLegNPV(),
        swap.fixedLegBPS(),
        swap.overnightLegBPS(),
        swap.fairRate(),
        swap.fairSpread(),
        swap.fixedRate(),
        swap.startDate().serialNumber(),
        swap.maturityDate().serialNumber(),
        len(swap.fixedLeg()),
    )
    ROWS.append(name + "\t" + "\t".join(format(value, ".17g") for value in values))
Path(__file__).with_name("reference.tsv").write_text("\n".join(ROWS) + "\n")
