# ABCD math and covariance

These additive Rust-only helpers cover Medium inventory rows **36 + 35**.
Existing models, defaults and bindings are unchanged. No calibration, LMM
wiring, complete market model or release is included.

## Shape and distinct defaults

`f(t) = (a + b*t)*exp(-c*t) + d`, with `f(t)=0` for finite `t<0`.

| Type | Default `(a,b,c,d)` |
| --- | --- |
| `math::abcdmathfunction::AbcdMathFunction` | `(0.002,0.001,0.16,0.0005)` |
| `termstructures::volatility::AbcdFunction` | `(-0.06,0.17,0.54,0.17)` |

Both provide `new`, `with_defaults`, `Default`, coefficient getters and checked
`value`. Math additionally exposes the free coefficient `validate` function.
`c` must be strictly positive; zero is not a supported constructor shortcut.
`d` and `a+d` must be non-negative. Negative `a` or `b` can be valid, but a
negative stationary minimum cannot. Finite coefficients are mandatory.
The negative-slope minimum is validated in the log domain to avoid overflow.
With `b<0,d=0`, a mathematically negative eventual tail is rejected even if
native exponential underflow would conceal it.

## Rate covariance

For fixing times `T,S` and observation time `u`:

- `instantaneous_covariance(u,T,S)` returns `f(T-u)*f(S-u)`.
- `covariance(t1,t2,T,S)` integrates that product to `min(t2,T,S)`.
- `variance(t1,t2,T)` is the **integrated** variance, not an average variance.
  Duration normalization is not supplied by these helpers.
- `AbcdSquared::new(a,b,c,d,T,S)` stores an immutable integrand. Despite its
  name, `T` and `S` need not be equal.

Finite signed times are allowed. Negative observation times are not silently
clipped at zero. At a fixing time the instantaneous value includes `f(0)`;
strictly after either fixing covariance is zero. Integrated zero-width and
already-cut-off intervals return zero. Reversed bounds return an error.

```rust
use libitofin::termstructures::volatility::AbcdFunction;

let shape = AbcdFunction::with_defaults()?;
let covariance = shape.covariance(0.0, 1.0, 2.0, 3.0)?;
let variance = shape.variance(0.0, 1.0, 2.0)?;
assert!(covariance >= 0.0 && variance >= 0.0);
# Ok::<(), libitofin::errors::QlError>(())
```

## Numerical boundaries and oracle

Integration uses analytic decaying exponential moments in local interval
coordinates. For `c*duration < 0.5`, it integrates the product of local
33-term Taylor expansions of the volatility values, preserving near-zero
initial values with `expm1`. At this threshold the omitted exponential term
is below `0.5^33/33!`; this is a fixed analytic expansion, not adaptive
quadrature. It does not subtract
large native primitives or replace positive small `c` with exactly zero.
This avoids the original formula's long-maturity exponential overflow and
small-decay cancellation in ordinary representable arithmetic.

All numerical queries return `QlResult`. Non-finite inputs/results, negative
computed values, unrepresentable lags, durations, moments or intermediate
expressions return errors, even when exact real arithmetic could yield a
finite answer. There is no arbitrary precision, negative-variance clamp or
universal relative-accuracy guarantee near zero; ordinary floating-point
underflow can return zero. Cutoff shortcuts validate every input first.

Original source pin: `9863b578af0caa4cecabf697196533e84a8308b6`.
The directly exported math/value/covariance operations are checked against the
compiled QuantLib **1.43** wheel, a separate revision. `AbcdSquared` delegates
to the native covariance operation but is not exported by that wheel.
High-precision/quadrature checks cover small decay and short intervals.
See `tests/fixtures/medium2_abcd/README.md` for reproduction.

This is the indexed fork's bounded subset, not the complete native ABCD API.
Primitive, derivative, maximum, rolling-window coefficients, normalized
volatility and calibration APIs are intentionally deferred. Native primitive
normalization/support quirks and native extrema conventions are consequently
not exposed or claimed as covered.
