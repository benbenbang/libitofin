# Hull-White forward process bindings

`HullWhiteForwardProcess` is available in C, Go and Python as a standalone
short-rate process under a mutable forward measure. It is not a calibrated
Hull-White model, bond pricer, tree or simulation engine.

## Construction and lifetime

- Python: `itofin.processes.HullWhiteForwardProcess(curve, a, sigma)`.
- Go: `session.NewHullWhiteForwardProcess(curve, a, sigma)`.
- C: `itofin_hw_forward_process_new`, query and horizon setter.
- `a` and `sigma` are finite and nonnegative. Zero values are supported.
- The exact supplied live curve handle is retained, not only its current link.
  Releasing the curve wrapper does not release the process dependency.
- The initial short rate `x0` is captured at construction. Curve quote updates
  change `alpha`, drift and conditional expectation, but never reset `x0`.
- The horizon starts at zero. `set_forward_measure_time` /
  `SetForwardMeasureTime` validates finite nonnegative values and uses the
  native notifying setter. Invalid updates preserve the old horizon.
- No public curve-relink setter is introduced. Rebuild the process when changing
  a facade's curve identity. Native retained relinkable handles remain live.

## Queries

Python exposes `x0`, `a`, `sigma`, `forward_measure_time`, `alpha`, `b`, `m_t`,
`drift`, `diffusion`, `expectation`, `std_deviation`, `variance` and `evolve`.
Go uses `X0`, `A`, `Sigma`, `ForwardMeasureTime`, `Alpha`, `B`, `MT`, `Drift`,
`Diffusion`, `Expectation`, `StdDeviation`, `Variance` and `Evolve`.

All times are process years, not dates. Times and intervals must be finite and
nonnegative; state and Gaussian draw must be finite. Times may exceed the
forward-measure horizon. `B(t,maturity)` accepts maturity before `t` and returns
negative values; `M_T(s,t,maturity)` requires `s <= t`, but its maturity may
precede either endpoint. Its endpoints are absolute times, not durations.

A zero transition interval returns the input state and zero variance after
validating inputs. Diffusion is the constant supplied volatility. Curve bounds
and errors are preserved, without requesting extrapolation. A missing native
curve link makes queries requiring curve access fail while captured `x0`, kernels,
diffusion and variance remain available.

C query kinds are exactly 0 `x0`, 1 `a`, 2 `sigma`, 3 horizon, 4 `alpha(t)`,
5 `B(t,dt)`, 6 `M_T(t,x,dt)`, 7 drift, 8 diffusion, 9 expectation,
10 standard deviation, 11 variance, 12 evolve. For kinds 5/6, `dt` is an
absolute maturity and kind 6 also uses `x` as its second endpoint. Unused inputs
are ignored. Outputs are written only on success. Context ownership/thread and
pointer requirements remain those of the [C/Go contract](https://github.com/benbenbang/libitofin/blob/main/docs/go-binding-contract.md).
Go objects belong to their session, reject mixed sessions and require explicit
Close. Python instances are unsendable and retain native dependencies.

## Numerical and oracle qualifications

The facade delegates to existing Rust process mathematics unchanged. Stable
zero/small-`a` algebra for `B`, `alpha` and `M_T` is retained; upstream QuantLib
zero-`a` drift is nonfinite and small-`a` expressions suffer cancellation.
These are not repaired or imitated by the facade. The original
Ornstein-Uhlenbeck variance approximation below `sqrt(f64::EPSILON)` remains.
At the cutoff itself, its direct `1-exp` expression also retains cancellation;
this binding does not change that numerical branch or claim exact Decimal parity.
For example, at `a = sqrt(epsilon)`, `sigma = 0.017`, `dt = 0.9`, the retained
direct expression gives `0.0002600999963395298`, while a 90-digit independent
integral gives `0.0002600999965117872511`. The difference also propagates to
standard deviation and evolution; this is not a new binding correction.

Independent focused replay of the actual Rust C facade passed 2,340 baseline
acceptance checks; the original Python facade passed 2,253. The two facades share
2,154 scalar observations, all bit-exact. Separate strict source-compatible
high-precision diagnostics retain 18 cutoff differences per facade across
variance, standard deviation and evolution. Full exact-OU diagnostics separately
retain 54 differences per facade, 18 for each of those outputs, additionally
exposing the below-cutoff approximation. These are actual retained Rust-facade
observations, not acceptance-tolerance waivers or claims inferred from the
separate compiled upstream wheel.

Source reference: QuantLib `9863b578af0caa4cecabf697196533e84a8308b6`,
`ql/processes/hullwhiteprocess.{hpp,cpp}`. Existing core fixtures distinguish that
checkout from the separately compiled QuantLib 1.43 wheel and independent
high-precision limits. Bindings do not establish full QuantLib/model parity.

[Python example](https://github.com/benbenbang/libitofin/blob/main/example/python/hw_forward_process.py)
and [Go example](https://github.com/benbenbang/libitofin/tree/main/sdk/go/examples/hw_forward_process).
