# Monte Carlo variance swaps

`MCVarianceSwapEngine` prices the same [spot-start variance swap](variance-swaps.md)
using **integrated log-price diffusion squared**. It is not a historical realized
variance or discrete squared-return estimator.

## Estimator and supported market

For each path, the engine integrates `sigma(t, S(t))²` from zero to the final
grid time and divides by that time. `sigma` is the retained generalized
Black-Scholes process's **log-price** diffusion, not arithmetic diffusion
`sigma × S`. The pinned native floating-grid trapezoidal rule selects the path
state by truncating `t / dt`; its integration intervals can differ from the
path-step count by floating-point rounding. Numerical fixture parity preserves
that distinction rather than substituting integer-node indexing.

- Supported automatic volatility dispatch: `BlackConstantVol` and
  `BlackVarianceCurve<Linear>` through its local-volatility curve.
- Rust retains existing explicit external local-volatility handles, with reference,
  state and diffusion validation. No new external-local-vol facade is added to
  Go or Python.
- Other variance-curve interpolators and volatility surfaces are not silently
  converted to local volatility. Standalone arithmetic GBM, Heston, jump and
  GJR processes are not accepted as this engine's process.
- Start must equal current Settings evaluation date and **all three** market
  reference dates at pricing. No forward-start contracts, historical fixings,
  accrued variance, observation calendars or discrete-monitoring controls.
- Paths require positive finite states and finite nonnegative log diffusion;
  nonfinite intermediate values or outputs error rather than publish NaN/Inf.

Strike `0.04` is annualized **variance**, equivalent to `20%²`. Notional is cash
per whole variance unit. Long NPV is
`discount(maturity) × notional × (variance − strike)`; short reverses its sign.
The risk-free curve's reference-to-maturity year fraction determines the grid
horizon. See the instrument page for ownership and start/expiry boundaries.

## Configuration and results

Exactly one grid mode and one sampling mode must be supplied. Contradictory
selectors error. An optional checked maximum is valid in either sampling mode.

| Input | Contract |
| --- | --- |
| `steps` | Positive explicit path-step count |
| `steps_per_year` | Positive density; count is `max(trunc(density × maturity_time), 1)` |
| `samples` | Fixed sample count of at least two |
| `tolerance` | Finite positive **absolute annualized-variance standard-error** target, not cash or relative error |
| `max_samples` | Tolerance mode: defaults to 50,000, at least 1,023. Fixed mode: optional validated upper bound, at least `samples` |
| `seed` | `0` randomizes; `1..2³²−1` is reproducible and restarts on recalculation |
| Resource caps | At most 100,000 steps / 1,000,000 samples; conservative `(2 × steps + 2) × sample_budget ≤ 100,000,000` |

Fixed mode always runs the requested sample count; a supplied maximum validates
the bound but does not act as a stopping criterion. Its work budget uses the
requested count. Tolerance mode starts with 1,023 paths, then adapts native-sized batches.
It reports the **actual** count used. Reaching the budget without meeting the
target errors; it does not return an unconverged price. Only PseudoRandom is
supported: no antithetics, Brownian bridge, control variates or quasi-MC modes.

| Result | Meaning |
| --- | --- |
| `variance()` | Mean annualized path statistic |
| `samples()` | Actual completed path count |
| `variance_error()` | Nonnegative standard error in annualized-variance units |
| `error_estimate()` | Native signed cash error: `discount × notional × position_sign × variance_error`; negative for shorts |
| `option_weights()` | Empty for MC, not a replication portfolio |

Sampling errors exclude **time discretization and integration bias**. At constant
volatility, the mathematical path statistic is `sigma²`; floating integration and
centered sample statistics can introduce tiny roundoff error. Sampling error can
be zero even on a coarse grid. That does not certify the grid or a more general
volatility model.
Do not use the signed cash error as an unsigned confidence-interval half-width.

The finite option-strip engine has separate tail/strike-spacing bias and a
[native nonzero-dividend limitation](variance-swaps.md#native-nonzero-dividend-limitation).
Compare the two engines only in valid zero-dividend, dense-strip limits, not as
an arbitrary finite-strip equality assertion.

## Paired examples

The complete examples share spot `100`, rate `3%`, no dividends, constant
volatility `20%`, one year, `252` steps, `1,023` samples and seed `42`. Variance
is approximately `0.04`, with approximately zero NPV at strike `0.04`.

=== "Rust"

    ```rust
    use libitofin::pricingengines::MCVarianceSwapEngine;

    let engine = MCVarianceSwapEngine::new(
        process, Some(252), None, Some(1023), None, None, 42,
    )?;
    ```

    [Complete Rust market, attachment and result example](https://github.com/benbenbang/libitofin/blob/main/crates/libitofin/examples/mc_variance_swap.rs).

=== "Python"

    ```python
    from itofin.pricingengines import MCVarianceSwapEngine

    engine = MCVarianceSwapEngine(process, steps=252, samples=1023, seed=42)
    swap.set_engine(engine)
    print(swap.variance(), swap.samples(), swap.variance_error(), swap.error_estimate())
    ```

    Tolerance mode replaces `samples=1023` with `tolerance=1e-5, max_samples=50000`.
    [Complete Python example](https://github.com/benbenbang/libitofin/blob/main/example/python/mc_variance_swap.py).

=== "Go"

    ```go
    engine, err := session.NewMCVarianceSwapEngine(process, itofin.MCVarianceSwapConfig{
        Steps: 252, Samples: 1023, Seed: 42,
    })
    if err != nil { panic(err) }
    defer engine.Close()
    if err := swap.SetMCEngine(engine); err != nil { panic(err) }
    ```

    Go/C zero-valued option selectors mean unset; Python uses `None` and rejects
    explicit zero grid/sample selectors or tolerance. Tolerance mode uses
    `AbsoluteTolerance: 1e-5, MaxSamples: 50000` instead of `Samples`.
    [Complete Go example with all results and cleanup](https://github.com/benbenbang/libitofin/blob/main/sdk/go/examples/mc-variance-swap/main.go).

## Ownership and recalculation

The engine retains its process, while the instrument retains its engine and
Settings. C handles and Go objects are context/session-owned; close Go objects
before their session. Python objects are thread-confined. Market edits invalidate
cached prices; nonzero-seed recalculation restarts the stream. A failed live
calculation does not expose stale or partial sampling results. Replication engines
do not provide MC sample/error statistics.

Default maturity-day NPV and cash error estimate are zero; variance, sample count,
variance error and weights are unavailable. Included maturity-day events still
require positive pricing time. Changing the reference-date-event flag after an
expired cache requires explicit recalculation because that setter does not notify.

[Pinned source-built native fixtures](https://github.com/benbenbang/libitofin/tree/main/sdk/go/testdata/mc-variance-swap)
include the original time-dependent 90-day native case, not only flat volatility.
