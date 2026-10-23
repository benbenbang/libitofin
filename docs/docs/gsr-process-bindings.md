# Constant-parameter GSR process bindings

C, Go and Python expose the existing standalone `GsrProcess`. This is a
one-dimensional Gaussian state process under a forward measure, not a fitted
short-rate model, calibration engine, curve or pricing engine.

## Construction and domain

- Python: `itofin.processes.GsrProcess(times, vols, reversions, t_measure)`.
- Go: `session.NewGsrProcess(times, vols, reversions, tMeasure)`.
- C: `itofin_gsr_process_new` copies three pointer/count arrays.
- Volatility count must equal grid-time count plus one. Reversion count must be
  one or equal volatility count. All repeated volatility and reversion values
  must be identical. Varying parameter grids are unsupported.
- Grid times must be finite, nonnegative and strictly increasing. Volatility is
  finite and nonnegative. Reversion is finite and may be negative or zero.
- The forward-measure horizon is finite and nonnegative. Grid boundaries do not
  constrain horizon updates. Zero horizon and zero volatility are valid.
- Query times and step endpoints must be within `[0,T]`. States and Gaussian
  draws must be finite; step durations must be finite and nonnegative.
- Date conversion is not supported. Times are process years.

## Queries and horizon changes

The facades expose `x0`, `sigma(t)`, `reversion(t)`, `y(t)`, `g(t,end)`, drift,
instantaneous diffusion, conditional expectation, standard deviation, variance
and evolution with one standard Gaussian draw. The initial state is zero.
The affine kernel requires ordered absolute endpoints, not a duration.
Zero reversion uses the exact continuous limit, without a small-reversion cutoff.

Python `forward_measure_time()` and `set_forward_measure_time(horizon)`, and Go
`ForwardMeasureTime()` and `SetForwardMeasureTime(horizon)`, access and update
the horizon through the core notifying setter. A valid update immediately changes
drift and transition means. An invalid update preserves the previous horizon.
The process has no exposed observer-registration API.

The existing Rust process intentionally differs from QuantLib 1.43 in two
edge cases: it does not apply a small-reversion cutoff, and horizon changes
update transition means as well as drift. QuantLib 1.43 retains its original
transition-mean horizon after the inherited setter changes the reported
horizon. These bindings preserve Rust behavior, not that stale-horizon defect.

C uses `itofin_gsr_process_query`: kinds 0 initial state, 1 horizon, 2 sigma,
3 reversion, 4 y, 5 G, 6 drift, 7 diffusion, 8 expectation, 9 standard deviation,
10 variance and 11 evolve. Kind 5 treats its `dt` argument as the absolute end
time. Unused arguments are ignored. Scalar and handle outputs are written only
on success. `itofin_gsr_process_set_forward_measure_time` updates the horizon.

## Ownership and errors

Construction copies inputs, so subsequent caller-array changes have no effect.
C handles belong to their creating context and OS thread and must be released.
Go methods are serialized by the owning session and support concurrent callers;
close both process and session explicitly. Python objects are unsendable and
retain their own process state. Native domain failures propagate as binding
errors. Unrepresentable nonfinite results are rejected rather than returned.

## Examples

```python
from itofin.processes import GsrProcess

p = GsrProcess([], [0.03], [0.0], 10.0)
mean = p.expectation(2.0, 0.1, 0.8)
p.set_forward_measure_time(5.0)
changed_mean = p.expectation(2.0, 0.1, 0.8)
assert mean != changed_mean
```

The Go example is in `sdk/go/examples/gsr_process`. These additive bindings do
not extend the supported core model or varying-grid scope.
