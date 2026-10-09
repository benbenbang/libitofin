# FX forwards

`libitofin::fxforward::FxForward` is an additive Rust-core convenience API for
an outright FX forward. Existing APIs, defaults and bindings are unchanged.
It is not the complete QuantLib `FxForward` instrument/engine interface.

## Contract and dates

- Spot is an **immutable ExchangeRate snapshot**, target/quote units per
  source/base unit. A derived rate contributes its descriptive endpoint rate,
  not its conversion lineage. This API does **not** observe a live spot quote.
- Supply a fixed `spot_settlement_date` and an unadjusted `delivery_date`.
  There is no automatic calendar adjustment or default two-day spot lag.
- Base notional and strike must be finite and strictly positive. The contracted
  quote notional must also remain finite and positive after multiplication.
  Zero notional, negative notional and zero strike are rejected deliberately.
- Long receives base and pays quote at delivery; short reverses those cashflows.
- Both discount handles remain live: changes to curve quotes or handle relinks
  affect the next query. No pricing cache or observers are added by the forward.
- Each curve reference date must be on/before spot settlement. Reference dates
  can differ and need not equal the explicit evaluation date.

## Pricing

Write `D_b(d)` and `D_q(d)` for curve-reference discounts, `s` for spot
settlement, `T` for delivery, `S` for snapshot spot and `K` for strike:

\[
B=D_b(T)/D_b(s),\quad Q=D_q(T)/D_q(s),\quad F=S B/Q.
\]

The long quote-currency NPV follows QuantLib's actual engine operation order:

\[
\mathrm{NPV}_q=(N B-(N K)Q/S)S D_q(s).
\]

Short NPV is the opposite. `npv(evaluation_date)` returns a `Money` tagged with
spot's target currency, valued at the **quote curve reference date**, not at
`evaluation_date`. The evaluation argument controls **only expiry**:

| Boundary | Behavior |
| --- | --- |
| Delivery before evaluation | Tagged zero, without consulting curves |
| Delivery equals evaluation | Active; curve/discount checks still apply |
| Delivery equals spot settlement | Allowed; fair rate is the spot snapshot |
| Null date | Error |

`fair_forward_rate()` has no implicit clock or expiry check. It prices the fixed
contractual dates even after delivery, provided curves still cover them.
Settings cashflow/event flags do not change either query. Rebuild the forward
with a fresh spot snapshot/date when moving to a different market snapshot:
passing another evaluation date alone does **not** update spot or value dates.

Unlike the fork's simple `S*D_b(T)/D_q(T)` formula, normalization includes the
explicit spot-settlement discounts. Quote NPV uses its own settlement discount,
so unequal curve reference dates do not imply `NPV_quote = NPV_base * S`.

## Checked boundaries and numerical limits

- Identical/blank currencies, invalid dates, delivery before spot settlement,
  non-positive/non-finite spot, notional or strike are errors.
- Empty handles may be supplied for subsequent relinking; active valuation
  requires both curves. Currency association of untyped curve handles is the
  caller's responsibility.
- Discount queries do not force extrapolation. Each curve's explicit
  extrapolation setting remains respected.
- All queried discounts, forward-discount ratios and leg PVs must be finite and
  positive. Underflow to zero in these quantities is an error.
- Non-finite intermediate or final NPV is an error, even if symbolic
  rearrangement/cancellation could recover a finite result. IEEE underflow of
  the final signed NPV can yield zero. There is no arbitrary-precision promise.

Twelve direct compiled QuantLib 1.43 vectors and separate guard/relink tests
cover this scope. Fixture provenance and the distinct inspected source pin are
recorded in `crates/libitofin/tests/fixtures/medium2_fx_forward/README.md`.
No source-currency NPV, rolling schedule or new C/Go/Python facade is introduced.
