# Spot-start variance swaps

Price annualized variance using a **finite European call/put strip** on a
Black-Scholes market. This is the pinned QuantLib discrete log-payoff replication
rule, not a realized-variance estimator or an accuracy certificate.
For Monte Carlo integrated local variance on the same instrument, see
[MC variance swaps](mc-variance-swaps.md).

## Units and supported contracts

- Strike `0.04` means `0.20²`: annualized **variance**, not 4% volatility.
- Notional is cash per **one whole variance unit**, not vega-notional or
  percent-squared notional. With notional `50,000`, a variance change of `0.01`
  changes undiscounted long payoff by `500`.
- Long NPV is `discount(maturity) × notional × (variance − strike)`.
  Short reverses the sign. Strike and notional must be finite and positive.
- Annualization uses the risk-free curve's reference-to-maturity year fraction.
  Other curves retain their own day counters; calendar days/365 is not universal.

| Boundary | Contract |
| --- | --- |
| Start | Start equals current Settings evaluation date and reference date of risk-free, dividend and volatility curves at pricing |
| Monitoring | No realized fixings, accrued/seasoned variance, forward-start subtraction or discrete-sampling correction |
| Strip size | 2-4,096 raw strikes per side, at least two distinct strikes each |
| Strike boundary | Positive finite strikes; minimum call equals maximum put exactly |
| Tail extension | Finite `dk > 0`; lower put tail stays positive, both synthetic tails remain distinct and finite |
| Expiry | Default maturity-day NPV is zero; variance and weights are unavailable. Included maturity-day events still require positive pricing time |
| Results | Finite signed weights/variance/NPV are preserved, not clamped to zero |

Unsupported starts are rejected, including a previously valid contract made
seasoned by advancing Settings. Construct a new spot-start contract to rebase it.

## Replication and limits

The engine sorts calls ascending and puts descending, removes duplicates within
each side and retains the shared boundary on **both** sides. Returned weights
follow that order. Synthetic end strikes shape the final slopes but are not
purchased options. Rust/Go/Python results are independent owned copies.

For time `T` and shared strike boundary `f`, the replicated payoff is
`g(K) = (2/T) × ((K-f)/f − log(K/f))`. Weights are differences of adjacent
absolute secant slopes. Each purchased option is valued analytically on the
retained process. This is **piecewise-linear log-payoff replication**, not
trapezoidal `1/K²` quadrature or infinite-tail integration.

- Finite tails and strike spacing introduce truncation/discretization bias.
  Passing the size limits does not establish pricing accuracy.
- The engine does not interpolate option prices. Volatility interpolation,
  range and extrapolation follow the supplied volatility curve's contract.
- Nonfinite inputs, intermediate values or results error. Validation requires
  finite nonnegative **total option variance**. Existing BlackConstantVol squares
  a finite negative raw volatility quote, so that quote is accepted; this engine
  does not add a raw-volatility sign check. Extreme finite inputs need not yield
  useful accuracy.

### Native nonzero-dividend limitation

The native formula mixes a spot/risk-free correction with dividend-adjusted
option forwards. For constant rate `r`, dividend yield `q`, volatility `sigma`,
spot `S` and a complete continuous strip, its limit is

```text
sigma² + 2q + 2S/(T f) × (exp((r-q)T) - exp(rT))
```

The boundary-dependent term is essential. At `q=0` this reduces to `sigma²`;
with nonzero dividends it generally does not. Finite strips add further bias.
Do not interpret a negative finite result as a variance certificate or take its
square root blindly. Itofin preserves this native pricing convention.

## Paired examples

These examples share spot `100`, rate `3%`, dividend `0`, volatility `20%`,
Actual/365 Fixed and a one-year spot-start contract. Calls span `100..150` and
puts `50..100`, both in steps of `5`; `dk=5` gives synthetic ends `155` and `45`.
They inspect a bounded strip, not an asserted exact fair variance of `0.04`.

=== "Rust"

    ```rust
    use libitofin::instruments::VarianceSwap;
    use libitofin::position::Position;
    use libitofin::pricingengines::ReplicatingVarianceSwapEngine;

    let engine = ReplicatingVarianceSwapEngine::new(process, 5.0, &calls, &puts)?;
    let swap = VarianceSwap::new(Position::Long, 0.04, 50_000.0,
        today, today + 365, settings)?;
    ```

    [Complete market, engine attachment and results](https://github.com/benbenbang/libitofin/blob/main/crates/libitofin/examples/variance_swap.rs).

=== "Python"

    ```python
    from itofin.instruments import Position, VarianceSwap
    from itofin.pricingengines import ReplicatingVarianceSwapEngine

    engine = ReplicatingVarianceSwapEngine(
        process, list(range(100, 151, 5)), list(range(50, 101, 5)), dk=5.0
    )
    swap = VarianceSwap(Position.Long, 0.04, 50_000.0, today, today + 365, settings)
    swap.set_engine(engine)
    print(swap.variance(), swap.npv(), len(swap.option_weights()))
    ```

    [Complete Python example](https://github.com/benbenbang/libitofin/blob/main/example/python/variance_swap.py).

=== "Go"

    ```go
    engine, err := session.NewReplicatingVarianceSwapEngine(itofin.ReplicatingVarianceSwapEngineConfig{
        Process: process, Dk: 5, CallStrikes: calls, PutStrikes: puts,
    })
    if err != nil { panic(err) }
    defer engine.Close()
    swap, err := session.NewVarianceSwap(itofin.VarianceSwapConfig{
        Position: itofin.PositionLong, Strike: 0.04, Notional: 50_000,
        StartDate: today, MaturityDate: maturity, Settings: settings,
    })
    if err != nil { panic(err) }
    defer swap.Close()
    if err := swap.SetEngine(engine); err != nil { panic(err) }
    ```

    [Complete Go example with results and explicit cleanup](https://github.com/benbenbang/libitofin/blob/main/sdk/go/examples/variance-swap/main.go).

## Live markets and ownership

- Engine retains its process; swap retains engine and Settings. Closing input
  facade handles does not destroy dependencies retained by a surviving object.
- Market quote edits invalidate cached prices. This intentionally fixes pinned
  QuantLib's missing process registration: compare updated prices with native
  **explicit recalculation**, not native stale cache.
- Public Go/Python BlackScholesProcess currently takes a scalar spot. Existing
  quote-backed rate/dividend/volatility construction supports live updates;
  this feature does not add a public spot-quote facade.
- The reference-date-event flag setter does not notify observers. If it
  changes after an expired result is cached, explicitly recalculate the swap.
- A failed live calculation does not publish a stale result. Restoring valid
  quotes allows lazy recovery. Missing engine and unavailable Greeks/results
  error rather than fabricate values.
- C objects are context-owned; Go objects belong to one Session. Go calls are
  serialized, including count-and-copy weight reads. Close objects before their
  session. Python classes are thread-confined.

[Pinned native fixtures and regeneration](https://github.com/benbenbang/libitofin/tree/main/sdk/go/testdata/variance-swap)
record numerical provenance separately from these API boundaries.
