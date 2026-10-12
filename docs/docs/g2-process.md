# Bare-factor G2 process

`libitofin::processes::G2Process` simulates two correlated Gaussian factors:

```text
dx = -a x dt + sigma dW1
dy = -b y dt + eta dW2
corr(dW1, dW2) = rho
```

## Included

- Five scalar parameters, with zero initial factors.
- Exact conditional means, covariance and Gaussian step generation.
- Finite signed mean reversion, including zero and arbitrarily small values.
- Nonnegative volatility, including deterministic factors, and `rho` in `[-1,1]`.
- Zero steps and singular instantaneous correlations.
- The existing object-safe `StochasticProcess` interface and path generators.

## Rust example

```rust
use libitofin::{
    math::array::Array,
    processes::G2Process,
    stochasticprocess::StochasticProcess,
};

let process = G2Process::new(0.1, 0.01, 0.2, 0.008, -0.75)?;
let state = process.initial_values()?;
let next = process.evolve(0.0, &state, 0.25, &Array::from([0.5, -0.3]))?;
```

The last argument contains two independent standard-normal draws, not Brownian
increments already scaled by the square root of the time step.

## Exact transitions

For `I(k,dt) = (1-exp(-k*dt))/k`, with `I(0,dt) = dt`:

| Quantity | Formula |
| --- | --- |
| Mean first factor | `x * exp(-a*dt)` |
| Mean second factor | `y * exp(-b*dt)` |
| First variance | `sigma² * I(2a,dt)` |
| Second variance | `eta² * I(2b,dt)` |
| Cross covariance | `rho * sigma * eta * I(a+b,dt)` |

Integrated covariance is not generally the instantaneous covariance multiplied
by `dt`. For unequal reversion rates, the integrated correlation can be less
than one even when `rho` equals one. The lower-triangular step root reproduces
this exact covariance. Logarithmic scaling and `expm1` avoid cancellation and
unnecessary volatility-squared overflow. A factored small-step series and a hyperbolic covariance identity preserve the
second shock for nearly singular transitions. The independent-noise residual is
computed with `expm1` before the correlation rounds to one. A twelfth-order
midpoint difference of `log(sinh(z)/z)` is used when both `|a*dt|` and `|b*dt|`
are at most `0.125`; the remaining correlated regime uses a stable hyperbolic
identity or the logarithmic kernel ratio. Only bounded positive
log-correlation roundoff is clamped to zero.

## Validation and native reference

Constructor parameters, states and noise must be finite. Fallible dynamics
require two state/noise entries, nonnegative time and step, a finite endpoint,
and representable finite results. Invalid inputs and overflow return errors.
The inherited infallible `apply` remains ordinary component-wise addition.
Date-to-time conversion is unsupported. Parameters are immutable and there are
no market-data subscriptions.

Tracked regular-step fixtures come from QuantLib 1.43 with an empty yield-curve
handle. Algebraic and strict 90-digit Decimal root-regression tests cover zero volatility,
zero/small/signed reversion,
singular correlation and finite extreme scaling.

QuantLib's inspected instantaneous G2 diffusion and the comparison fork use
`rho * sigma` as the second factor's first loading. When volatilities differ,
that does not produce the required second variance or cross covariance. This
implementation deliberately uses `rho * eta` instead. Exact native transitions
are compared separately from that corrected instantaneous loading. Native
zero-step/zero-volatility and zero-reversion divisions are not imported.

## Not included

- A deterministic term-structure shift `phi(t)` or fitted G2 short-rate model.
- Interpreting `x+y` as a curve-fitted short rate.
- G2ForwardProcess, two-factor trees/lattices, calibration or pricing engines.
- New Python, C or Go bindings.

Existing APIs and models are unchanged. See [simulation](simulation.md) for
other processes and path-generation conventions.
