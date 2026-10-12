# Callable bond OAS sensitivities

Both `CallableFixedRateBond` and `CallableZeroCouponBond` expose
`effective_duration` and `effective_convexity` in Rust core. Existing APIs and
bindings are unchanged. These methods use an explicit immutable
`&TreeCallableFixedRateBondEngine`, with its Hull-White curve and time steps.
They do not require, replace or calculate through the installed instrument engine.

```rust
let duration = bond.effective_duration(
    spread, &engine, day_counter.clone(), compounding, frequency, None,
)?;
let convexity = bond.effective_convexity(
    spread, &engine, day_counter, compounding, frequency, Some(0.001),
)?;
```

- `spread` and `bump` are conventional OAS rates, not basis-point counts.
- `None` selects `h = 0.0002`, or two basis points. Each of `s`, `s+h` and `s-h`
  is independently converted using the requested day count and compounding.
- `P(s)` is the **clean settlement price**, per 100, from `clean_price_oas`.
- Duration: `(P(s-h) - P(s+h)) / (2 P(s) h)`.
- Convexity: `(P(s+h) + P(s-h) - 2 P(s)) / (P(s) h²)`.
- This clean-price denominator follows QuantLib's callable-bond methods.
  It is not dirty-price duration or convexity. Accrued interest therefore matters.
- A zero center price returns zero, after both bumped prices are evaluated.
- Nonfinite, nonpositive and unrepresentable bumps return errors. Pricing errors
  propagate; nonfinite results or arithmetic overflow/underflow are rejected.
- Successful and failed probes preserve the installed engine, its results,
  instrument cache, observer notifications and frozen state.

Native QuantLib 1.43 fixtures cover 36 fixed/zero, call/put/noncallable,
continuous/compounded/simple and default/custom-bump cases. Nonzero accrued
fixed-rate cases distinguish clean normalization from dirty normalization.
Settlement-normalized OAS quotes retain the underlying OAS method's documented
correction for nonzero settlement lag.
