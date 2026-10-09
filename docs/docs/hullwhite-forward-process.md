# Hull-White forward-measure process

`HullWhiteForwardProcess` adds Rust-core short-rate dynamics under a
`T`-forward measure. It is not a complete Hull-White pricing/calibration model.
Existing processes, engines, traits, defaults and bindings are unchanged.

```rust
use libitofin::handle::Handle;
use libitofin::interestrate::Compounding;
use libitofin::processes::{ForwardMeasureProcess1D, HullWhiteForwardProcess};
use libitofin::shared::{Shared, shared};
use libitofin::stochasticprocess::StochasticProcess1D;
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;

let curve = shared(FlatForward::with_rate(
    Date::new(9, Month::October, 2026), 0.03, Actual365Fixed::new(),
    Compounding::Continuous, Frequency::Annual,
));
let mut process = HullWhiteForwardProcess::new(
    Handle::new(curve as Shared<dyn YieldTermStructure>), 0.1, 0.01,
)?;
process.set_forward_measure_time(5.0)?;
let mean = process.expectation(1.0, 0.02, 0.7)?;
let variance = process.variance(1.0, 0.02, 0.7)?;
# Ok::<(), libitofin::errors::QlError>(())
```

## Contract

| Item | Behavior |
| --- | --- |
| Parameters | Finite nonnegative mean-reversion `a` and volatility `sigma` |
| Initial state | Curve instantaneous forward at zero, captured at construction |
| Curve | Retained live handle; quote changes and relinks notify observers |
| Horizon | Initially `0`; existing validated notifying setter accepts finite `T >= 0` |
| Time units | Years in the curve's reference-date/day-counter system |
| States | Finite signed short rates are supported |
| Time domain | Finite nonnegative times and steps; finite end time |
| Horizons before time | Allowed, matching native equations; `B` can be negative |
| Date conversion | Unsupported, matching the native process base |
| Extrapolation | No explicit request; the linked curve's own permission is honored |

`x0()` does not change after quote updates, relinks or an emptied handle.
`alpha`, drift and positive-step expectation read the current curve and fail
if it is unavailable. Diffusion and variance use only constant parameters.
The original instantaneous-forward helper internally extrapolates its small
finite-difference window after checking the requested time. Drift separately
requests `t + 0.0001`, so a non-extrapolating finite curve can reject drift near
its maximum time.

Zero steps preserve the state exactly and return zero variance without reading
curve forwards. Nonfinite shocks are still rejected for zero-step evolution.

## Equations and limits

The fitting shift is `alpha(t) = f(t,t) + sigma² B(0,t)² / 2`.
Drift follows the original finite-difference curve derivative with shift
`0.0001`, plus mean reversion and the `-sigma² B(t,T)` measure correction.
It is not an analytic derivative: interpolation knots can produce large drift
values, and forward extraction rounding is amplified by division by `0.0001`.

Expectation uses the exact Ornstein-Uhlenbeck mean, the fitting-shift change
and the integrated correction
`M_T(s,t,T) = sigma² ∫[s,t] exp(-a(t-u)) B(u,T) du`.
The public `m_t` helper requires `s <= t`; `b` and `m_t` return `QlResult`.

For every positive `a`, `B` uses `expm1`; zero `a` uses its algebraic limit.
If the product `a * duration` underflows to zero, that limit also preserves
subnormal durations. A stable equivalent factorization evaluates `M_T`:

` sigma² B(s,t) [B(t,T) + exp(-a(T-t)) B(s,t)/2] `

This intentionally avoids the original tiny-`a` cancellation and repairs the
native zero-`a` drift's division by zero. It also avoids the native epsilon
shortcut when tiny `a` multiplied by a large duration is material.

Variance retains the existing original OU rule: for `a < sqrt(f64::EPSILON)`,
it returns `sigma² dt`; otherwise it uses the native exponential expression.
The small-speed branch is an approximation and need not be accurate for very
large `dt` even when `a` is tiny. No existing OU behavior is changed.

This is finite `f64` arithmetic, not arbitrary precision. Intermediate
multiplication overflow or cancellation may return an error even when a
mathematically rearranged result would be finite. Ordinary floating-point
underflow is retained. Nonfinite returned process values are rejected.

## Validation and deferred work

Tracked tests cover seven direct compiled QuantLib cases, independent
80-digit small-`a` integrals, Simpson integration, live relinks/notifications,
horizon validation, zero steps and malformed/domain inputs. See the
[oracle provenance](https://github.com/benbenbang/libitofin/tree/main/crates/libitofin/tests/fixtures/medium2_hullwhite_forward).

The original source pin and compiled QuantLib 1.43 wheel are distinct.
Spot-measure `HullWhiteProcess`, Hull-White model calibration, bond-option
pricing, hybrid Heston/Hull-White dynamics and bindings are outside this slice.
