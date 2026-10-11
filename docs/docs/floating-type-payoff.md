# Floating-strike payoff

`FloatingTypePayoff` is an additive Rust-core port of QuantLib's payoff with
its strike supplied at evaluation time. It stores only the option type.

| Option | `value_with_strike(price, strike)` |
| --- | --- |
| Call | `max(price - strike, 0)` |
| Put | `max(strike - price, 0)` |

```rust
use libitofin::instruments::FloatingTypePayoff;
use libitofin::option::OptionType;

let call = FloatingTypePayoff::new(OptionType::Call);
assert_eq!(call.value_with_strike(120.0, 100.0), 20.0);
assert_eq!(call.value_with_strike(120.0, 130.0), 0.0);
```

## Supported contract

- The strike is independent for each evaluation. No persistent or default strike.
- Equality pays zero. There is no tolerance band.
- Negative prices and strikes are supported.
- Construction is infallible, matching existing payoff conventions.
- Subtraction and comparison follow the local QuantLib source contract. NaN remains NaN,
  negative intrinsic values become positive zero, and a negative zero intrinsic
  value is preserved. Equal infinities produce NaN.
- Implements `Payoff`, `TypePayoff`, `Clone`, `Copy`, `Debug`, `PartialEq` and `Eq`.
  The concrete `'static` type supports `Any` downcasting.
- Name: `FloatingType`. Description: `FloatingType Call` or `FloatingType Put`.

## One-input evaluation and pricing boundaries

**Do not call `Payoff::value(price)` on this payoff.** It cannot supply the strike
and always panics with `floating payoff not handled`, including through a trait
object. This deliberately unsupported operation mirrors QuantLib's exception and
the existing `NullPayoff` convention. Use the non-panicking two-input method.

This type does not implement `StrikedTypePayoff`. It cannot be passed to
`BlackCalculator::with_striked_payoff`, and no analytic visitor arms, floating-
strike instruments, pricing engines, C/Python/Go bindings or release are added.
Existing APIs, defaults and payoff behavior remain unchanged.

## Oracle and tests

The reference contract is `QuantLib/ql/instruments/payoffs.hpp` and `payoffs.cpp`.
The tracked fixture contains 56 direct calls to native QuantLib 1.43's SWIG
`FloatingTypePayoff(price, strike)`, including adjacent-strike prices, negative
inputs, subnormals, signed zeros, overflow, infinities and NaNs. The generator also
checks that native one-input evaluation throws the expected error. Native finite
and infinite nonzero results are compared by exact f64 bits; NaNs by
classification, without relying on a platform-specific NaN payload. The installed
wheel canonicalizes negative-zero results to positive zero, unlike the local
source's intrinsic-first `std::max`. Zero results are compared by numeric equality,
while separate exact-bit tests lock the source-faithful negative-zero behavior
consistent with the existing Rust `PlainVanillaPayoff`. The fixture retains the
actual native output bits; no numeric tolerance is used.

Regenerate using QuantLib 1.43:

```bash
python crates/libitofin/tests/fixtures/medium5_floating/generate.py
cargo test --locked -p libitofin --test medium5_floating
```

Additional tests cover changing strikes, vanilla equivalence, put-call parity,
metadata, trait-object dispatch, downcasting and explicit unsupported evaluation.
