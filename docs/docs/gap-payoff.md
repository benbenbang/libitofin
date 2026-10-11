# Gap payoff

`GapPayoff` is an additive Rust-core terminal payoff matching QuantLib's
`ql/instruments/payoffs.hpp` and `payoffs.cpp`.

| Type | Payment when active | Trigger |
| --- | --- | --- |
| Call | `S - K2` | `S - K1 >= 0` |
| Put | `K2 - S` | `K1 - S >= 0` |

- `strike()` returns trigger `K1`; `second_strike()` returns payment strike `K2`.
- The trigger is inclusive: payment at `S = K1` can be nonzero or negative.
- Active payments are not floored. Equal strikes reduce to a vanilla payoff
  for finite inputs.
- `Payoff`, `TypePayoff` and `StrikedTypePayoff` are implemented.
- Construction is infallible and inputs are not validated or clamped, consistent
  with existing payoff types. A NaN trigger difference is inactive; an active
  payment with a NaN second strike remains NaN.

```rust
use libitofin::instruments::GapPayoff;
use libitofin::option::OptionType;
use libitofin::payoff::Payoff;

let call = GapPayoff::new(OptionType::Call, 100.0, 110.0);
assert_eq!(call.value(95.0), 0.0);
assert_eq!(call.value(100.0), -10.0);
assert_eq!(call.value(120.0), 10.0);
```

## Scope

This is a scalar payoff, not a new exotic instrument or pricing engine.
Analytic engine visitor dispatch and BlackCalculator support are not added.
No Python, C or Go binding, existing API change, or release is included.
