# Exchange-rate chaining

`ExchangeRate::chain(&first, &second)` adds derived Rust-core rates without
changing direct constructors, direct conversion or binding facades.

```rust
use libitofin::currency::Currency;
use libitofin::exchangerate::{ExchangeRate, ExchangeRateType};
use libitofin::money::Money;

let eur_usd = ExchangeRate::checked_new(Currency::eur(), Currency::usd(), 1.2)?;
let usd_gbp = ExchangeRate::checked_new(Currency::usd(), Currency::gbp(), 0.8)?;
let eur_gbp = ExchangeRate::chain(&eur_usd, &usd_gbp)?;
assert_eq!(eur_gbp.rate_type(), ExchangeRateType::Derived);
let sterling = eur_gbp.exchange(&Money::new(Currency::eur(), 100.0))?;
assert_eq!(sterling.currency(), &Currency::gbp());
# Ok::<(), libitofin::errors::QlError>(())
```

## Orientation and ownership

For first rate `A -> B` with value `r1`, the first matching shared-currency rule
sets the derived endpoints and descriptive stored rate:

| Second rate | Derived endpoints | Stored rate |
| --- | --- | --- |
| `A -> C` | `B -> C` | `r2 / r1` |
| `C -> A` | `B -> C` | `1 / (r1 * r2)` |
| `B -> C` | `A -> C` | `r1 * r2` |
| `C -> B` | `A -> C` | `r1 / r2` |

Children are immutable, owned snapshots with shared nested lineage. The source
rates may be dropped, and further chaining and cloning retain the full path.
There is no exchange-rate manager, date lookup, rounding or automatic routing.

## Conversion is ordered, not flattened

- If the input currency matches either endpoint of the first child, conversion
  applies that child, then the second child. Otherwise it tries second, then first.
- Each child may itself be derived. Intermediate rounding is retained.
- A shared intermediate currency is not automatically a valid input: the first
  conversion may produce a currency that the second cannot convert.
- Equal or opposite currency pairs follow the same first-match rule. A chain with
  equal displayed endpoints is not an identity operation. The retained path can
  also accept a child endpoint not displayed by the derived rate.
- Consequently `rate()` describes the endpoints, but `exchange()` is not defined
  as multiplication or division by that single number.

## Numeric limits

- Chaining requires finite positive child rates and a finite positive computed
  stored rate. Unrelated currencies, invalid numbers, aggregate overflow and
  aggregate underflow to zero return `QlResult::Err`.
- Stored-rate formulas preserve QuantLib's operation ordering, including the
  product before reciprocal. They do not promise correctly rounded algebraically
  equivalent results across intermediate overflow/underflow.
- Input amounts must be finite; positive, negative and zero amounts are supported.
- Conversion checks each intermediate result. Overflow returns an error even when
  the flattened rate would produce a finite final value. Underflow can yield zero
  even when multiplication by the stored rate would give a nonzero result.
- QuantLib permits non-finite results; these checked differences deliberately
  preserve the parent Rust numeric contract.

## Evidence

Reference laws were inspected at QuantLib commit
`9863b578af0caa4cecabf697196533e84a8308b6`. Independent compiled **QuantLib 1.43**
observations cover every orientation, reversed arguments, nested and overlapping
pairs and numerical extremes. The wheel is not claimed to be a build of that
source revision. The portable generator and provenance notes are in
`crates/libitofin/tests/fixtures/medium_fx_chain/`; Rust tests need no Python runtime.
