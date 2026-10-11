# Bachelier asset ITM helper

Rust-core `bachelier_black_formula_asset_itm_probability` complements the
normal-model pricing functions. Existing pricing functions and bindings are
unchanged. Negative forwards and strikes are valid.

| Input | Meaning |
| --- | --- |
| `option_type` | Call or put |
| `strike`, `forward` | Finite levels, including negative rates |
| `std_dev` | Finite, non-negative normal volatility times square root of maturity |

## QuantLib's zero-volatility convention

| Standard deviation | Return value |
| --- | --- |
| Positive | `Phi(sign * (forward - strike) / std_dev)` |
| Exactly zero, including negative zero | `max(sign * (forward - strike), 0)` |

**The zero-volatility branch is intrinsic value, not a probability.** It may
exceed one. Exactly at the money it returns zero, whereas any positive standard
deviation gives one half. This deliberately preserves the QuantLib helper's
unusual discontinuity, rather than substituting an indicator or clamping it.

```rust
use libitofin::option::OptionType;
use libitofin::pricingengines::blackformula::bachelier_black_formula_asset_itm_probability;

let atm = bachelier_black_formula_asset_itm_probability(
    OptionType::Call, -0.01, -0.01, 0.005,
)?;
assert_eq!(atm, 0.5);
let intrinsic = bachelier_black_formula_asset_itm_probability(
    OptionType::Call, -2.0, 3.0, 0.0,
)?;
assert_eq!(intrinsic, 5.0);
# Ok::<(), libitofin::errors::QlError>(())
```

## Validation and numerical boundaries

- Unlike native QuantLib, this new fallible API rejects non-finite inputs.
- An unrepresentable zero-volatility intrinsic value returns an error.
- Ordinary positive-volatility calculations retain subtraction before division,
  avoiding cancellation from independently dividing close levels.
- Only when that difference overflows, division precedes subtraction. With
  forward `f64::MAX`, strike `-f64::MAX` and standard deviation `f64::MAX`, the
  standardized distance is two, not infinity. Native QuantLib overflows the
  difference and returns one, while this helper returns `Phi(2)`.
- A genuinely infinite standardized distance has a valid CDF endpoint, zero or
  one. No blanket probability clamp or existing-formula change is introduced.

## Reference coverage

The fixture generator calls native **QuantLib 1.43** for 30 ordinary-domain
cases. Two overflow-range cases use a separate 90-digit Decimal normal-density
integral. The local C++ source oracle is `blackformula.cpp:950` at commit
`9863b578af0caa4cecabf697196533e84a8308b6`; it is distinct from the native wheel.
Tests cover parity, ATM, negative rates, tails, signed zero, adjacent values,
finite-domain errors, zero-volatility discontinuity and overflow boundaries.
