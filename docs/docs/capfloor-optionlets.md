# Cap/floor optionlets

`CapFloor::optionlet(index)` extracts one coupon's caplet, floorlet or collarlet
from an Ibor or overnight cap/floor. Indices are zero-based.

| Property | Extracted instrument |
| --- | --- |
| Type | Same cap, floor or collar type as the parent |
| Coupon | Same shared coupon, not a rebuilt copy |
| Strikes | One indexed strike for each relevant side |
| Settings | Same evaluation-date and fixing-history handle |
| Engine | Not inherited; attach explicitly |
| Cached NPV | Not copied |
| Invalid index | `QlResult` error, including an empty leg |

Strike vectors are padded by the existing constructors before extraction, so a
short strike vector still selects the parent's effective strike for that coupon.
A pure cap does not copy unused floor strikes, and a pure floor does not copy
unused cap strikes.

## Pricing the decomposition

```rust
let mut caplet = cap.optionlet(0)?;
caplet.base_mut().set_pricing_engine(engine.clone());
let value = caplet.npv()?;
```

Import `libitofin::instrument::Instrument` for engine attachment and pricing.
With the same supported cap/floor engine, the sum of the extracted instruments'
NPVs equals the parent's NPV. Attaching a different engine is allowed, but then
that decomposition need not hold.

The shared coupon and settings preserve ordinary quote, daily/historical fixing
and evaluation-date notifications. Extraction does not implement recursive
`deepUpdate`, change coupon cache behavior or add implied-volatility inversion.

## Reference and validation

- Contract: repository `QuantLib/ql/instruments/capfloor.cpp:195-209`, source
  checkout `9863b578`.
- Numerical oracle: separately installed QuantLib-Python **1.43**, not a build of
  that repository checkout. Its Python wrapper does not expose `optionlet`, so
  the fixture generator performs the exact source operation: one retained coupon
  and the indexed cap/floor strikes, with a freshly attached engine.
- Tracked fixtures cover Ibor and overnight cap/floor/collar, non-flat and padded
  strikes, gearing/spread, Bachelier normal volatility and displaced Black.
- Tests check individual oracle prices at absolute tolerance `1e-12`, summed
  NPVs, shared ownership, missing-engine and invalid-index errors, market/fixing
  updates and expiry.

This is additive Rust-core functionality. Existing constructors, defaults and
bindings are unchanged; no new Python, C or Go facade is added.
