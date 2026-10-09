# Bond Z-spread analytics

The Rust-core `BondFunctions` API adds three wrappers over the existing
`CashFlows` parallel zero-rate spread implementation:

| Method | Result |
| --- | --- |
| `dirty_price_at_z_spread` | Settlement dirty price per 100 of current notional |
| `clean_price_at_z_spread` | Dirty price minus the bond's settlement accrued interest |
| `z_spread` | Spread that reprices a `BondPrice::Clean` or `BondPrice::Dirty` quote |

## Conventions

- The supplied `Handle<dyn YieldTermStructure>` remains live. Each call uses
  its current link and values; no independent frozen curve is created.
- `None` settlement uses the bond's settlement-date calculation and its own
  `Settings`, not global state. Explicit settlement overrides that default.
- NPV is normalized to settlement, not the curve reference date.
- Coupon, ex-coupon and amortization handling delegates to existing `CashFlows`
  and `Bond` behavior. Current outstanding notional supplies the per-100 scale.
- The wrappers request exclusion of settlement-date flows. Existing
  `include_todays_cash_flows` settings can override cash-flow occurrence rules
  when settlement is the evaluation date, as in QuantLib.
- Spread compounding and frequency are explicit. The original curve supplies
  its day counter; these methods do not add a separate spread day counter.
- `z_spread` defaults: accuracy `1e-10`, maximum evaluations `100`, guess `0.0`.
  Spread accuracy is not a guaranteed absolute price-error bound. Request tighter
  accuracy when a tighter price residual is needed.

## Validation and limits

- A missing notional schedule or zero outstanding notional is an error. Prices
  and inversions at or after the final redemption are not tradable.
- Non-finite spreads, quoted prices, accrued/scaled prices and scaled NPV targets
  are rejected. Solver accuracy must be finite and positive; the evaluation
  limit must be nonzero and the guess finite.
- Curve range errors, invalid rate conventions, malformed cash-flow errors and
  solver nonconvergence propagate as `QlResult` errors.
- Quote normalization avoids avoidable `NPV * 100` overflow. Genuine cash-flow
  or target-NPV overflow remains an error, not a supported infinite price.
- This is a Rust-only addition. It does not add binding facades, a pricing
  engine, credit calibration or instrument-level `Bond` overloads.

## Evidence

`tests/medium_bond_zspread.rs` embeds 48 independently generated compiled
QuantLib 1.43 cases with `1e-10` price and spread tolerances. Cases cover
continuous/compounded spreads, positive/negative spreads, irregular periods,
ex-coupon accrual, amortization and settlement before/on/after a payment.
Additional tests cover all five compounding modes at zero spread, bond-local
settings, solver errors, malformed coupons and a representable `1e308` notional.

Reproduction instructions and the inspected source revision are in
`crates/libitofin/tests/fixtures/medium_bond_zspread/README.md`.
