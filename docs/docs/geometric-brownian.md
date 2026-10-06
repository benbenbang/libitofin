# Standalone geometric Brownian motion

`GeometricBrownianMotionProcess` is the named scalar QuantLib-compatible process

```text
dX = mu × X × dt + volatility × X × dW
```

Its drift and diffusion are arithmetic-state coefficients. Its finite transitions
use the shared **additive Euler** discretization, not exact-lognormal evolution.

## Transition contract

| Operation | Result |
| --- | --- |
| Initial state | Copied `initial`, returned by `x0()` / Go `X0()` |
| Parameters | Copied immutable `mu` and `volatility` |
| `drift(t, x)` | `mu × x` |
| `diffusion(t, x)` | `volatility × x` |
| `expectation(t, x, dt)` | `x + (mu × x) × dt` |
| `variance(t, x, dt)` | `(volatility × x)² × dt` |
| `std_deviation(t, x, dt)` | `(volatility × x) × sqrt(dt)` |
| `evolve(t, x, dt, dw)` | Expectation plus signed deviation times `dw` |

`dw` is a standard Gaussian draw, not a pre-scaled Brownian increment. These
methods do not sample a random number themselves. Time and `dt` are finite and
nonnegative; initial/state/mu/draw may be finite signed values, including zero.
Volatility must be finite and nonnegative. Nonfinite output/intermediate overflow
errors. For validated finite inputs, a zero step preserves the exact signed state
and returns zero variance/deviation **without evaluating coefficients**. This
safety shortcut avoids native exceptional `Inf × 0` behavior at extreme finite
coefficients; it does not relax input validation.

- **Signed deviation is intentional**: state `−100`, volatility `0.20` and
  step `0.25` give deviation `−10`, while variance is `100`.
- A large negative draw can cross zero: state `100`, mu `0.05`, volatility
  `0.20`, step `1` and draw `−6` produce `−15`.
- These are Euler moments, not the exact exponential GBM moments. No implicit
  date-to-time conversion, market quotes or engine connection is added.

## Choose the right API

| Need | Use |
| --- | --- |
| Named scalar process and native Euler transitions | `GeometricBrownianMotionProcess` |
| Seeded exact-lognormal GBM paths, including correlated assets | [Simulation helpers](simulation.md): Python `simulate_gbm`, Go `SimulateGBM` |
| Risk-neutral market process for an option or variance-swap engine | Existing Black-Scholes process and curves |

This standalone process is **not** an alias for the simulation helpers and cannot
be supplied to `MCVarianceSwapEngine`. That engine integrates a Black-Scholes
process's log-price diffusion, not this arithmetic `volatility × x` diffusion.

## Paired examples

=== "Rust"

    ```rust
    use libitofin::processes::GeometricBrownianMotionProcess;
    use libitofin::stochasticprocess::StochasticProcess1D;

    let process = GeometricBrownianMotionProcess::new(100.0, 0.05, 0.20)?;
    assert_eq!(process.expectation(0.0, 100.0, 0.25)?, 101.25);
    assert_eq!(process.std_deviation(0.0, -100.0, 0.25)?, -10.0);
    assert_eq!(process.evolve(0.0, 100.0, 1.0, -6.0)?, -15.0);
    ```

    [Complete Rust accessor and transition example](https://github.com/benbenbang/libitofin/blob/main/crates/libitofin/examples/geometric_brownian.rs).

=== "Python"

    ```python
    from itofin.processes import GeometricBrownianMotionProcess

    process = GeometricBrownianMotionProcess(100.0, 0.05, 0.20)
    assert process.expectation(0.0, 100.0, 0.25) == 101.25
    assert process.std_deviation(0.0, -100.0, 0.25) == -10.0
    assert process.evolve(0.0, 100.0, 1.0, -6.0) == -15.0
    ```

    [Complete Python example](https://github.com/benbenbang/libitofin/blob/main/example/python/geometric_brownian.py).

=== "Go"

    ```go
    process, err := session.NewGeometricBrownianMotionProcess(100, 0.05, 0.20)
    if err != nil { panic(err) }
    defer process.Close()
    deviation, err := process.StdDeviation(0, -100, 0.25)
    if err != nil { panic(err) }
    fmt.Println(deviation)
    ```

    [Complete Go example with cleanup](https://github.com/benbenbang/libitofin/blob/main/sdk/go/examples/geometric-brownian/main.go).

C objects belong to their context, Go objects to one Session. Close Go objects
before closing their session. Python objects are thread-confined and own their
immutable scalar process; no external market owners are required.

[Pinned independent native fixtures](https://github.com/benbenbang/libitofin/tree/main/sdk/go/testdata/geometric-brownian)
cover signed states, zero steps and zero-crossing transitions.
