# Snapshot-spot FX forwards

`itofin.instruments.FxForward` and Go `Session.NewFxForward` wrap the existing
Rust forward, without a new pricing engine or core numerical changes.

## Contract

- Spot and strike are **quote currency per one base currency**. Long receives
  base and pays quote; short reverses both legs.
- Supply distinct currencies, a positive finite spot, base notional and strike,
  and two live discount curves. Currency identity remains native name-based.
- Spot is copied, not observed. Both dates are explicit, fixed and unadjusted:
  delivery must be on or after spot settlement. No calendar, rolling spot lag,
  live FX quote, Money facade or automatic market-date refresh is included.
- Both curves are retained after wrapper release. Quote-backed curve updates
  affect every pricing query. Reconstruct the forward to replace a curve:
  this bounded facade has no general curve relink setter.
- `fair_forward_rate()` / `FairForwardRate()` has no implicit evaluation date
  and does not apply expiry.
- `npv(evaluation_date)` / `NPV(evaluation)` returns a scalar **quote-currency
  amount at the quote curve reference date**, not at the evaluation date.
  `quote_currency()` / `QuoteCurrency()` identifies that denomination.
- Evaluation controls expiry only. Delivery strictly before evaluation returns
  zero without querying curves; delivery on evaluation remains active,
  regardless of global reference-event/cashflow settings.
- Both curve reference dates must be on or before spot settlement. Curve domain,
  nonpositive/nonfinite discounts and unrepresentable cash calculations raise
  errors. Pricing does not enable extrapolation automatically.

For settlement `s` and delivery `T`, let `B = D_base(T)/D_base(s)` and
`Q = D_quote(T)/D_quote(s)`. The fair rate is `spot * B / Q`. Long NPV is
algebraically `notional * (fair - strike) * D_quote(T)`, subject to the core's
explicit finite-intermediate checks and conversion ordering.

## Python

```python
from itofin.indexes import Currency
from itofin.instruments import FxForward, Position
from itofin.termstructures import FlatForward
from itofin.time import Date, DayCounter

dc = DayCounter.actual365_fixed()
reference = Date(1, 1, 2025)
forward = FxForward(
    Currency.eur(), Currency.usd(), 1.1,
    FlatForward(reference, .03, dc), FlatForward(reference, .05, dc),
    1_000_000., 1.12, Date(3, 1, 2025), Date(3, 1, 2026), Position.Long,
)
quote_currency_npv = forward.npv(reference)
```

Stored-field accessors expose spot, strike, base notional, both dates, position
and copied currencies. Python objects are thread-confined. C handles are typed,
context/thread-confined with outputs written only on success. Go serializes
calls on its session worker and rejects mixed sessions. Close each Go object
and session explicitly; returned currency copies are independently owned.

Tests use independently derived flat-curve covered-interest-parity algebra,
not a claim of full QuantLib instrument/engine parity.
