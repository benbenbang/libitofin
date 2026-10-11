# Percentage-strike payoff

`PercentageStrikePayoff` is a Rust-core scalar payoff, matching QuantLib's
percentage-strike contract. Moneyness is a dimensionless ratio: `0.9` means
90 percent, not an absolute strike or the number `90`.

| Type | Payoff at underlying price `S` |
| --- | --- |
| Call | `S * max(1 - m, 0)` |
| Put | `S * max(m - 1, 0)` |

The factor is floored **before** multiplication. Negative underlying prices
can produce negative payoffs. At `m = 1`, finite prices pay signed zero;
infinite prices yield NaN from infinity times zero.

```rust
use libitofin::instruments::{PercentageStrikePayoff, StrikedTypePayoff};
use libitofin::option::OptionType;
use libitofin::payoff::Payoff;

let payoff = PercentageStrikePayoff::new(OptionType::Call, 0.5);
assert_eq!(payoff.strike(), 0.5);
assert_eq!(payoff.value(80.0), 40.0);
assert_eq!(payoff.value(-80.0), -40.0);
```

## API and numerical boundaries

- Construction is infallible, like the existing scalar payoff types.
- No positivity, finiteness or range validation is added.
- `strike()` returns moneyness, not a currency amount.
- Implements `Payoff`, `TypePayoff`, `StrikedTypePayoff` and `Any` downcasting.
- Arithmetic follows QuantLib's subtraction, `std::max` and multiplication
  order. NaN moneyness propagates, and negative zero is not normalized.
- Names and descriptions follow the existing Rust formatting convention:
  `PercentageStrike Call, 0.5 strike`.

## Oracle and scope

The source oracle is `QuantLib/ql/instruments/payoffs.hpp` and `payoffs.cpp`
at checkout `9863b578af0caa4cecabf697196533e84a8308b6`.
The checked-in TSV contains 44 evaluations of the actual native QuantLib
**1.43** object, generated separately by `generate.py`. Non-NaN results are
compared bitwise, including signed zero; NaNs are compared by classification,
not platform-dependent payload bits. The installed native version is not
claimed to be the same build as the repository's pinned source checkout.

This addition does not enable a new pricing engine. `BlackCalculator` rejects
this dynamic payoff type explicitly. Bindings, percentage-strike engines and
structured instruments remain outside scope; existing payoff behavior is unchanged.
