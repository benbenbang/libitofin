# Black-Scholes theta helper

`pricingengines::black_scholes_theta` is an additive Rust-core helper. It recovers
annual theta from a `GeneralizedBlackScholesProcess` and a supplied present value,
delta and gamma:

```text
theta = r * value - (r - q) * spot * delta
        - 0.5 * local_vol^2 * spot^2 * gamma
```

## Market conventions

| Input | Query |
| --- | --- |
| Spot | Current process state-variable quote |
| `r` | Current risk-free curve, continuous zero rate at time zero |
| `q` | Current dividend curve, continuous zero rate at time zero |
| Local volatility | Current process local volatility at `(0, current spot)` |

- The time-zero zero-rate query uses the curve's standard short-time convention,
  currently `1e-4` years. This is not a maturity rate or the process drift's
  forward-rate sample.
- An external local-volatility handle takes precedence over Black volatility.
- Handles are read each call. Quote notifications and relinks invalidate the
  process's existing derived-local-volatility cache.
- No automatic extrapolation is enabled. Existing curve range checks and their
  explicit extrapolation settings apply.
- The supplied value/delta/gamma are not repriced. Callers must refresh them to
  match the process market. Values and Greeks can be signed; negative rates and
  zero local volatility are supported.

## Rust usage

```rust
use libitofin::errors::QlResult;
use libitofin::pricingengines::{black_scholes_theta, default_theta_per_day};
use libitofin::processes::GeneralizedBlackScholesProcess;

fn theta(
    process: &GeneralizedBlackScholesProcess,
    value: f64,
    delta: f64,
    gamma: f64,
) -> QlResult<(f64, f64)> {
    let annual = black_scholes_theta(process, value, delta, gamma)?;
    Ok((annual, default_theta_per_day(annual)))
}
```

## Errors and limits

- Returns `QlResult`: malformed or unreadable market inputs are not silently
  converted to zero.
- Rejects non-finite value/delta/gamma/rates/local volatility, non-positive spot,
  negative local volatility and a non-finite floating-point formula result.
- Empty handles, invalid quotes, curve range errors and unsupported derived local
  volatility propagate as errors. The process currently derives local volatility
  from constant Black volatility and linear Black variance curves only; use an
  explicit local-volatility handle for other supported local-volatility models.
- This is checked evaluation in ordinary floating-point source order, not an
  arbitrary-precision identity. Intermediate overflow or extreme cancellation
  can make an otherwise mathematically meaningful calculation return an error.
- QuantLib's helper does not validate these numerical inputs. The checks are an
  intentional Rust boundary hardening.
- `default_theta_per_day` is unchanged: it divides by 365 and still propagates
  IEEE NaNs/infinities. No existing engines are migrated to the new helper.
- No new C, Go or Python facade and no new pricing engine are included.

## Independent reference

The formula and market queries were inspected in `ql/pricingengines/greeks.cpp`
at QuantLib revision `9863b578af0caa4cecabf697196533e84a8308b6`.

The independent compiled oracle is PyPI QuantLib 1.43, not a build claimed to
match that source revision. Its wrapper does not export the helper directly.
Four compiled analytic European option theta values validate the PDE identity
using compiled NPV/delta/gamma. Additional compiled process queries validate
live quote updates, nonflat curves and an external local-volatility override.
The tracked generator under `crates/libitofin/tests/fixtures/medium_bs_theta`
records that distinction, the source hash and all observations.
