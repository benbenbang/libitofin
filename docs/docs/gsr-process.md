# Constant GSR state process

`GsrProcess` provides the additive Rust-core, constant-parameter subset of
QuantLib's Gaussian short-rate state process under a `T`-forward measure.
It is a state process, not a calibrated interest-rate model.

```rust
use libitofin::processes::{ForwardMeasureProcess1D, GsrProcess};
use libitofin::stochasticprocess::StochasticProcess1D;

let mut process = GsrProcess::new(vec![], vec![0.01], vec![0.03], 30.0)?;
let mean = process.expectation(2.0, 0.005, 1.0)?;
let variance = process.variance(2.0, 0.005, 1.0)?;
process.set_forward_measure_time(20.0)?;
```

## Contract

| Item | Supported behavior |
| --- | --- |
| Initial state | Zero |
| Volatility | Constant, finite and nonnegative, including zero |
| Reversion | Constant, finite and signed, including zero |
| Measure maturity | Finite and nonnegative, mutable with observer notification |
| Time domain | `0 <= t <= T`; transition start and end must lie in this range |
| Transition | Exact Gaussian mean, variance, standard deviation and evolution |
| Grid shape | `vols.len() = times.len()+1`; reversion count one or volatility count |
| Grid parameters | Repeated identical constants only; varying vectors are rejected |
| Grid times | Finite, nonnegative and strictly increasing; redundant for constants |
| Date conversion | Unsupported; pass process-year times directly |

All states and Gaussian increments must be finite. Negative/nonfinite steps,
out-of-domain endpoints and nonfinite numerical results return errors.
Repeated valid horizon setters notify observers even when unchanged. Invalid
setters preserve the old horizon and send no notification. Horizon changes
immediately affect drift and expectation but not conditional variance. Constant
parameter grid boundaries do not constrain the mutable measure maturity.

## Dynamics and numerical behavior

With `B(h) = (1-exp(-a h))/a`, the dynamics are:

- `y(t) = sigma² B(2t)/2`.
- `G(t,u) = B(u-t)`.
- `drift(t,x) = y(t) - sigma² G(t,T) - a x`.
- `variance(dt) = sigma² B(2dt)/2`.
- For `h=dt`, `u=t+h`, `e=exp(-a h)`, the conditional mean is
  `e x + B(h) e y(t) - sigma² B(h) B(T-u) (1+e)/2`.

Exponential-minus-one algebra preserves small nonzero reversions. Zero reversion
uses the continuous limits, without QuantLib's native `abs(a)<1e-4` truncation.
Variance includes logarithmic fallback when intermediate scale products would
underflow or overflow. Subnormal/underflow results follow binary64 arithmetic;
nonfinite results or transition-mean intermediates return errors, not NaN.

Independent checked-in fixtures evaluate the original unreduced QuantLib mean
integrals at 110-digit Decimal precision, including positive, negative, zero and
very small signed reversions. Tests also cover Gaussian transition composition,
constant grids, horizon updates, invalid domains, and extreme variance scaling.

## Not included

- Piecewise-changing volatility or reversion.
- GSR model fitting, term structures, calibration, numeraires or pricing engines.
- Reference-date/day-counter conversion or new C, Go and Python binding APIs.

Existing APIs and defaults are unchanged.
