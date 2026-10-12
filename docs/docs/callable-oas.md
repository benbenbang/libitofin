# Callable-bond OAS

Both `CallableFixedRateBond` and `CallableZeroCouponBond` expose Rust-core
`clean_price_oas` and `oas` methods. These are additive analytics over the
existing Hull-White callable tree, without new bindings or model dispatch.

## Explicit, immutable pricing engine

Pass `&TreeCallableFixedRateBondEngine` to either method. Analytics do not need
an installed engine and never replace it, modify engine arguments/results,
notify observers, invalidate a bond cache or clear its freeze state. Each price
probe builds an independent lattice from the supplied engine's model.

The model's curve is the sole source of maturity zero rates, spread conversion
and settlement discounts. There is no independently supplied curve that could
accidentally disagree with the tree's discounting curve. Existing public
`CallableBondArguments` fields and `PricingEngine` requirements are unchanged.

| Method | Inputs after the bond | Output |
| --- | --- | --- |
| `clean_price_oas` | Spread, engine, day counter, compounding, frequency, settlement | Clean price per 100 |
| `oas` | Clean price, engine, day counter, compounding, frequency, settlement, accuracy, evaluation limit, guess | Conventionally quoted spread |

- Spreads use decimal rate units: `0.01` is 100 basis points.
- Price spreads and returned OAS use the supplied day counter, compounding and
  frequency. All five compounding conventions are supported by rate conversion.
- Solver `guess` and `accuracy` are **continuous short-rate spread units**, not
  basis points or the output compounding convention. Initial search step: `0.001`.
- `max_evaluations` strictly bounds actual lattice price probes, including root
  search and the solver's final confirmation probe.
- `None` settlement uses a checked calendar advance from evaluation date,
  bounded by the issue date. `Some(date)` is an exact quote date, not an input
  date to be advanced. It must be nonnull, no earlier than issue or curve
  reference date, and strictly before redemption.
- Explicit settlement can precede evaluation date when it remains within those
  bounds. Settlement-day flows and exercises are excluded regardless of the
  non-notifying today/reference-date inclusion flags, as in ordinary arguments.

## Spread and price conventions

A conventional spread is converted at contractual maturity: add it to the
model curve's maturity zero rate, then convert the spreaded and base rates
separately to continuous compounding. Their difference is added to every
short-rate lattice discount. This does not shift the model curve or rebuild a
spreaded curve and refit Hull-White to that curve.

The settlement quote is:

`clean = NPV / base_discount(settlement) / notional(settlement) * 100 - accrued`

Inversion solves against the corresponding dirty settlement quote and converts
its continuous root back to the requested spread convention. The residual is
normalized per 100, avoiding false zero roots at tiny face amounts. Coupons owned at
settlement, ex-coupon exclusions and clean exercise accrued interest retain
existing callable-bond conventions. Zero-coupon accrued interest is zero.

## Native oracle distinction and validation

QuantLib's tree engine provides both reference-date NPV and settlement value,
but its OAS helper uses NPV directly as though it were already a settlement
value. This addition intentionally corrects that normalization for nonzero
settlement lag. Prices match the native helper at curve-reference settlement;
otherwise the independently normalized native reference is
`(native_clean + accrued) / base_discount(settlement) - accrued`.

Tracked QuantLib 1.43 fixtures cover fixed/zero products, nonflat curves,
settlement lags 0/2/10, positive/negative/zero spreads, five compounding
conventions and Actual/360 versus Actual/365 Fixed. Tests cover round trips,
explicit coupon/exercise settlement, inclusion flags, cached/frozen state,
zero-spread compatibility, face scaling from `1e-250` to `1e250`, invalid
conventions/dates and failed inversion.

General short-rate models, soft calls, spread-curve fitting and new bindings
remain outside this feature's scope.
