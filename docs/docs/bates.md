# Bates stochastic volatility with jumps

`BatesProcess` combines Heston stochastic variance with independent,
constant-intensity lognormal jumps. `BatesModel` holds eight calibrated
parameters. `BatesEngine` prices European plain-vanilla calls and puts.

## Parameters and limits

| Parameter | Meaning | Domain |
| --- | --- | --- |
| `v0` | Initial variance | Finite, positive |
| `kappa` | Variance mean-reversion speed | Finite, positive |
| `theta` | Long-run variance | Finite, positive |
| `sigma` | Volatility of variance | Finite, positive |
| `rho` | Spot/variance correlation | Finite, in `[-1, 1]` |
| `lambda` | Expected jumps per year | Finite, nonnegative |
| `nu` | Mean logarithmic jump multiplier | Finite |
| `delta` | Standard deviation of logarithmic jumps | Finite, nonnegative |

The spot drift is compensated by
`lambda * (exp(nu + delta**2 / 2) - 1)`. Unrepresentable jump moments or
compensation are errors. Zero intensity gives the Heston limit; zero jump
volatility allows a deterministic jump multiplier. The Feller condition is
not mandatory.

This initial process is a **pricing-only market carrier**. Generic drift,
diffusion and evolution are explicitly unsupported in Rust, and there is
no Bates simulation facade. It is not accepted by the Heston path simulator.
Deterministic-intensity and double-exponential models, finite-difference Bates
pricing and additional evolution schemes remain out of scope.

The engine reuses the Heston characteristic function and Gauss-Laguerre
integration, adding the compensated lognormal-jump exponent in the two
Gatheral probability integrals. Integration orders are `1..192`; Python
defaults to `144`. The engine reports **NPV only**: requesting a missing
Greek returns an error, not zero. American and other non-European exercises
are not supported.

## Price an option

=== "Python"

    ```python
    --8<-- "example/python/bates_option.py"
    ```

=== "Go"

    ```go
    --8<-- "sdk/go/examples/bates_option/main.go"
    ```

=== "Rust"

    ```rust
    --8<-- "crates/libitofin/examples/bates_option.rs"
    ```

The process retains live spot and yield-curve inputs. The model retains the
market, the engine retains the model and the option retains its engine.
Updating a quote or model parameter invalidates the cached option value.
Go objects must belong to the same session; closing the session ends access.

## Inspect and calibrate

Process getters expose the eight parameters, current spot, initial
`[spot, variance]` state and date-to-time conversion. Python uses `lambda_()`
because `lambda` is a keyword; Go uses `Lambda()`.

`model.params()` / `model.Params()` and `set_params` / `SetParams` use this
order, which differs from the process constructor:

```text
theta, kappa, sigma, rho, v0, nu, delta, lambda
```

Parameter replacement validates the entire array before mutation. Invalid
lengths, nonfinite values and domain errors leave the current model unchanged.

Calibration uses existing `HestonModelHelper` instruments with a **Bates**
engine, existing optimization methods and `EndCriteria`. Constraint, weights
and fixed-parameter options follow the same contract as Heston calibration,
but the fixed mask has **eight entries** in the order above. Model getters
read fitted values after calibration. Calibration fits risk-neutral prices;
it does not infer physical return forecasts.

- Weights must be finite and nonnegative; omitted weights default to one.
- Nonfinite final residuals or costs are errors, not successful fits.
- Calibration is stateful, not transactional: a failed fit may retain trial
  parameters and the installed helper engine. Only `set_params` is atomic.

## Numerical evidence

The [independent oracle](https://github.com/benbenbang/libitofin/blob/main/sdk/go/testdata/bates-oracle.md)
records QuantLib price and calibration provenance, quadrature orders and
fixed tolerances. Exact zero-intensity cases use independent Heston reference
prices because upstream Bates model constraints exclude zero intensity.
Finite quadrature orders need not agree with one another; reference tests
compare the same documented order.
