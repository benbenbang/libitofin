# Markov-functional state process

`MfStateProcess` is an additive, Rust-only standalone Gaussian state process.
It does not implement the full Markov-functional interest-rate model, curve
mapping, calibration, pricing engines, or binding APIs.

## Contract

| Property | Behavior |
| --- | --- |
| Initial state and drift | Zero |
| Conditional expectation | Current state, independent of reversion |
| Bucket boundaries | Right-continuous: a boundary selects the next volatility |
| Signed reversion `a` | Any finite value; it scales variance, not the drift |
| Native diffusion | Raw bucket `sigma(t)`, without `exp(a t)` |
| Transition variance | Integral of `sigma(s)^2 exp(2 a s)` over the interval |
| Empty bucket-time grid | Native unit-volatility variance, ignoring its sole supplied volatility |
| Native epsilon branches | `abs(a) < f64::EPSILON` means zero; `dt < f64::EPSILON` means zero variance |
| State updates | Exact Gaussian mean plus standard deviation times `dw` |

Constructor inputs use `Array`. Times must be finite, nonnegative and strictly
increasing. There must be one more volatility than boundary times, including
one volatility for an empty grid. Volatilities must be finite and nonnegative.
The process owns immutable arrays, with read-only accessors and no public setters.

```rust
use libitofin::math::array::Array;
use libitofin::processes::MfStateProcess;
use libitofin::stochasticprocess::StochasticProcess1D;

let process = MfStateProcess::new(
    0.1, Array::from([1.0, 2.0]), Array::from([0.2, 0.3, 0.1]),
)?;
let variance = process.variance(0.5, 0.0, 2.0)?;
let next_state = process.evolve(0.5, 0.0, 2.0, -0.7)?;
# Ok::<(), libitofin::errors::QlError>(())
```

## Validation and numerical safety

- Dynamics reject negative/nonfinite times, nonfinite states and increments,
  overflowing end times, nonfinite variances and overflowing evolved states.
- Nonzero-reversion integration uses anchored `expm1` to avoid subtracting nearly
  equal exponentials. Checked log-domain fallback prevents intermediate scaling
  overflow and loss through subnormal factors. Representable underflow is zero.
- Endpoints use native binary64 `t0 + dt`. If that sum rounds to `t0`,
  variance is zero, except the native empty-grid zero-reversion branch returns
  `dt` directly. Use time scales where the endpoint increment is representable.
- Raw `diffusion` and exact variance intentionally differ for nonzero reversion.
  Use the process transition methods, not an Euler reconstruction from diffusion.
- Validation of nonfinite/negative inputs is stricter than the original C++ API.
  Native bucket and epsilon conventions are retained apart from documented
  numerical stabilization. Existing process defaults remain unchanged.
- Dedicated tests use 51 actual pinned-original C++ observations and 15 independent
  80-digit source-law references. Near-zero/extreme cases test numerical accuracy,
  not replication of native floating-point cancellation artifacts.
