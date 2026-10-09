"""Print Rust vectors from compiled QuantLib 1.43 FX-forward instruments."""

import QuantLib as ql

assert ql.__version__ == "1.43"
today = ql.Date(15, ql.June, 2026)
ql.Settings.instance().evaluationDate = today
cases = [
    (0, 0, 0, 365, 0.02, 0.05, 1.1, 1.15, 1_000_000.0),
    (0, 0, 2, 365, 0.02, 0.05, 1.1, 1.15, 1_000_000.0),
    (-10, -20, 2, 365, 0.02, 0.05, 1.1, 1.15, 1_000_000.0),
    (-20, -10, 10, 400, -0.03, 0.01, 0.8, 0.9, 250_000.0),
    (0, 0, 0, 0, 0.02, 0.05, 1.1, 1.15, 1_000_000.0),
    (-10, 0, 20, 20, 0.04, -0.02, 1.3, 1.25, 100.0),
    (0, 0, 2, 30, -0.02, -0.04, 150.0, 148.0, 10_000.0),
    (0, 0, 2, 730, 0.1, 0.2, 0.006, 0.007, 1_000.0),
    (-365, -100, 0, 366, 0.05, 0.05, 1.2, 1.2, 500.0),
    (1, 2, 3, 365, 0.03, 0.07, 1.1, 1.2, 1_000_000.0),
    (0, 0, 0, 365, -0.05, 0.07, 1.0, 1.0, 1e-100),
    (0, 0, 0, 365, 0.05, -0.07, 1.0, 1.0, 1e100),
]
for case in cases:
    br, qr, settlement, delivery, rb, rq, spot, strike, notional = case
    base = ql.YieldTermStructureHandle(ql.FlatForward(today + br, rb, ql.Actual365Fixed()))
    quote = ql.YieldTermStructureHandle(ql.FlatForward(today + qr, rq, ql.Actual365Fixed()))
    engine = ql.DiscountingFxForwardEngine(base, quote, ql.QuoteHandle(ql.SimpleQuote(spot)))
    forward = ql.FxForward(notional, ql.EURCurrency(), ql.USDCurrency(), strike,
                           today + delivery, False, settlement, ql.NullCalendar())
    forward.setPricingEngine(engine)
    assert forward.settlementDate() == today + settlement
    row = (*case, forward.fairForwardRate(), forward.npvTargetCurrency())
    print("(" + ", ".join(repr(x) for x in row) + "),")
