# OIS payer and receiver selection

`MakeOis::with_type` adds a Rust-core builder option for the fixed-leg side.

| Type | Fixed leg | Overnight leg |
| --- | --- | --- |
| `SwapType::Payer` | Pay | Receive |
| `SwapType::Receiver` | Receive | Pay |

The existing default remains **Payer**. Repeated calls use the last value.

## Usage

```rust
use libitofin::instruments::SwapType;

let receiver = builder.with_type(SwapType::Receiver).build()?;
```

Here `builder` is an existing `MakeOis` with its index, settings and optional
fixed rate. Use `fixed_vs_floating().swap_type()` to inspect the chosen side.

## Pricing contract

- The setter changes only the side: schedules, notional, day counts, overnight
  spread, fixing behavior and discount-curve selection are unchanged.
- For identical inputs, reversing the side reverses total and each leg's NPV
  and signed BPS. Fair fixed rate and fair overnight spread remain the same.
- With `fixed_rate = None`, the temporary fair-rate swap also uses the chosen
  side, and the final swap is built at that fair rate.
- Both sides retain the existing engine error propagation. Empty curves and
  missing fixings are not silently accepted for valuation.
- Handles and settings remain shared, preserving ordinary repricing on
  notifications, including curve relinks and evaluation-date updates.

## Validation and scope

Whole-instrument fixtures use independent native QuantLib 1.43 OIS valuations
on distinct nonflat forecast/discount curves. Tests cover both sides, fair-rate
fill, EOM/stubs, payment lag, PV/BPS signs, fair-rate/spread rebuilds, default
compatibility, repeated setter calls, relinks and empty-curve recovery.

This is additive Rust-core scope. It does not add `receive_fixed`, per-leg
schedule overrides, a custom-engine builder option, telescopic/observation
features, arithmetic averaging, or Python/C/Go bindings. Existing guarded
unsupported options remain guarded; no release or version change is required.
