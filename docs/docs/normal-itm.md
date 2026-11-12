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
| Exactly zero, including negative zero | `1` if `sign * (forward - strike) > 0`, else `0` |

**The zero-volatility branch is the in-the-money indicator** of QuantLib v1.44
(`blackformula.cpp:970`), the `std_dev -> 0` limit off the money. Exactly at the
money it returns zero, whereas any positive standard deviation gives one half.
QuantLib 1.43 and earlier returned the intrinsic value `max(sign * (forward -
strike), 0)` here, which is not a probability.

```rust
use libitofin::option::OptionType;
use libitofin::pricingengines::blackformula::bachelier_black_formula_asset_itm_probability;

let atm = bachelier_black_formula_asset_itm_probability(
    OptionType::Call, -0.01, -0.01, 0.005,
)?;
assert_eq!(atm, 0.5);
let in_the_money = bachelier_black_formula_asset_itm_probability(
    OptionType::Call, -2.0, 3.0, 0.0,
)?;
assert_eq!(in_the_money, 1.0);
# Ok::<(), libitofin::errors::QlError>(())
```

## Validation and numerical boundaries

- Unlike native QuantLib, this new fallible API rejects non-finite inputs.
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
cases; the two in-the-money zero-volatility rows are pinned to the v1.44 rule
instead. Two overflow-range cases use a separate 90-digit Decimal normal-density
integral. The local C++ source oracle is `blackformula.cpp:950` at commit
`9863b578af0caa4cecabf697196533e84a8308b6`; it is distinct from the native wheel.
Tests cover parity, ATM, negative rates, tails, signed zero, adjacent values,
finite-domain errors, the zero-volatility indicator and overflow boundaries.
