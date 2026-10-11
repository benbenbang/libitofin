# Asset-or-nothing payoff

`AssetOrNothingPayoff` is an additive Rust-core terminal payoff matching
QuantLib's `ql/instruments/payoffs.hpp` and `payoffs.cpp`.

| Type | Payment when active | Trigger |
| --- | --- | --- |
| Call | Underlying price `S` | `S - K > 0` |
| Put | Underlying price `S` | `K - S > 0` |

- At `S = K`, both pay zero. There is no tolerance band.
- The payment is the underlying price, not a fixed cash amount.
- `Payoff`, `TypePayoff` and `StrikedTypePayoff` are implemented.
- Construction is infallible, consistent with existing payoff types. Inputs are
  not validated or clamped. Negative active prices retain their negative payment.
- A NaN trigger difference is inactive, including equal infinite price and
  strike; otherwise IEEE floating-point arithmetic is preserved.

```rust
use libitofin::instruments::AssetOrNothingPayoff;
use libitofin::option::OptionType;
use libitofin::payoff::Payoff;

let call = AssetOrNothingPayoff::new(OptionType::Call, 100.0);
assert_eq!(call.value(110.0), 110.0);
assert_eq!(call.value(100.0), 0.0);
```

## Scope

This is a scalar payoff, not a new exotic instrument or pricing engine.
Analytic engine visitor dispatch and BlackCalculator support are not added.
No Python, C or Go binding, existing API change, or release is included.
