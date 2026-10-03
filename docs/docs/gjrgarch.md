# GJR-GARCH process and paths

GJR-GARCH couples spot returns and variance with an asymmetric leverage effect.
The shared Rust process is exposed through C, Go and Python. This is a stochastic
process and simulation API, not the historical `Garch11` estimator. Calibrated
models and European engines are documented separately in
[GJR-GARCH pricing](gjrgarch-pricing.md) (#1165, unreleased).

## Parameters and units

| Input | Meaning |
| --- | --- |
| `spot` | Positive initial spot; the process retains a live quote |
| `daily_variance` | Nonnegative daily initial variance, not volatility |
| `omega` | Nonnegative daily variance intercept |
| `alpha`, `beta`, `gamma` | Daily GJR coefficients |
| `lambda_` / Go `Lambda` | Finite risk-premium parameter |
| `days_per_year` | Positive annualization factor, conventionally 252 |
| `horizon` | Nonnegative time in years |

All numbers must be finite. This library requires `alpha >= 0`, `beta >= 0`
and `alpha + gamma >= 0`. These are explicit supported-domain restrictions:
the upstream process itself does not validate parameters, and its calibration
model has a different constraint contract. Zero initial variance and intercept
are supported deterministic boundary cases.

State component zero is spot; component one is **annualized variance**.
Initial variance is `days_per_year * daily_variance`; the drift intercept is
`days_per_year**2 * omega`. Do not pass annualized variance as the daily input.

Persistence is `beta + alpha * (1 + lambda_**2) + gamma * q3`, where
`q3 = lambda_ * normal_pdf(lambda_) + (1 + lambda_**2) * normal_cdf(lambda_)`.
With a positive intercept, a finite stationary mean variance requires persistence
below one; finite-horizon simulation does not impose this condition.
No stationary-moment claim is made for other parameters.
Unrepresentable coefficients or simulated states raise errors.

## Supported schemes

The implementation follows the executable
[pinned QuantLib process](https://github.com/lballabio/QuantLib/blob/9863b578af0caa4cecabf697196533e84a8308b6/ql/processes/gjrgarchprocess.cpp).
It uses `2 + 4 * lambda_**2` in the diffusion, not the header's fourth-power typo.

| Python scheme | Go constant / C value | Negative variance handling in evolution |
| --- | --- | --- |
| `PartialTruncation` | `GJRPartialTruncation` / 0 | Raw variance in drift; zero diffusion |
| `FullTruncation` | `GJRFullTruncation` / 1 | Positive part in drift/diffusion; raw state retained |
| `Reflection` | `GJRReflection` / 2 | Absolute variance in drift/diffusion and state base |

Python defaults to `FullTruncation`; select the Go scheme explicitly.
Unknown schemes are rejected. Raw variance output can be negative under every
scheme: none guarantees a positive next Euler state. Do not silently clip the
returned state. Reflection's infinitesimal diffusion uses a signed square root
for negative variance, whereas its evolution uses a positive square root.
At zero elapsed time a negative Reflection state becomes its absolute value.

## Seeded paths

=== "Python"

    ```python
    import itofin

    config = dict(
        spot=100.0, daily_variance=0.04 / 252.0,
        risk_free_rate=0.05, dividend_yield=0.02,
        omega=2e-6, alpha=0.03, beta=0.90, gamma=0.04, lambda_=0.1,
        days_per_year=252.0, horizon=1.0, steps=252, paths=8, seed=42,
        scheme="FullTruncation",
    )
    full = itofin.simulate_gjr(**config)
    terminal = itofin.simulate_gjr(**config, terminal_only=True)
    assert full.shape == (8, 253, 2)
    assert terminal.shape == (8, 2)
    assert (terminal == full[:, -1, :]).all()
    ```

=== "Go"

    ```go
    config := itofin.GJRConfig{
        Spot: 100, DailyVariance: 0.04 / 252,
        RiskFreeRate: 0.05, DividendYield: 0.02,
        Omega: 2e-6, Alpha: 0.03, Beta: 0.90, Gamma: 0.04, Lambda: 0.1,
        DaysPerYear: 252, Horizon: 1, Steps: 252, Paths: 8, Seed: 42,
        Scheme: itofin.GJRFullTruncation,
    }
    full, err := itofin.SimulateGJR(config)
    if err != nil { return err }
    fmt.Println(full.Paths, full.Times, full.Assets)
    ```

=== "Rust"

    ```rust
    --8<-- "crates/libitofin/examples/gjr_paths.rs"
    ```

The normal stream is MT19937 plus inverse-normal transformation, consumed in
path, step, spot-factor, variance-factor order. Each step consumes two draws,
including zero-horizon runs. Seed zero is rejected. Identical inputs use the
same Rust kernel in all bindings; terminal values exactly match full-path tails.
Increasing path count preserves the existing path prefix.

Full layout is `[path, time, component]`, including time zero; terminal layout
is `[path, component]`. Python returns owned C-contiguous NumPy float64 arrays.
Go returns owned `Simulation.Values`, with `Assets=2` and row-major indexing.
Zero horizon repeats the initial positive-spot/nonnegative-variance state.

The default Python/Go limit is 16,777,216 output values (128 MiB).
`max_output_values=0` / Go `MaxOutputValues=0` selects that limit; positive values
override it. Negative limits, invalid dimensions, arithmetic overflow and native
allocation failures are errors. Terminal mode reduces output memory, not the
number of simulated steps.

The live process retains its market inputs; stateless paths instead use copied
flat rates and parameters. Closing external Go input handles does not release
the process's retained inputs. Calls through a closed process/session fail.

::: itofin.simulate_gjr

See the [process API reference](api/processes.md#itofin.processes.GJRGARCHProcess).
