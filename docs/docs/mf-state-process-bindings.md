# Standalone MF state process

`itofin.processes.MfStateProcess` in Python and `Session.NewMfStateProcess` in
Go expose the existing immutable Gaussian state process. This is **not** a fitted
Markov-functional interest-rate model, calibration API or pricing engine.

Pass signed finite reversion, strictly increasing finite nonnegative bucket times
and finite nonnegative raw volatilities. Volatility count must exceed time count
by one. Constructors and grid getters copy arrays. Times are process years,
not dates. There are no parameter setters or retained curve dependencies.

## Numerical conventions

- `x0` and drift are zero. Expectation is the unchanged initial state.
- Bucket boundaries are right-continuous: a query exactly at a boundary uses
  the following bucket. The last volatility applies beyond the last boundary.
- `diffusion` returns raw `sigma(t)`, **not** `sigma(t) exp(a t)`.
- Variance integrates `sigma(s)^2 exp(2 a s)` over the transition interval.
- An empty time grid deliberately uses **unit volatility for variance**, even
  when the sole raw volatility is zero. Diffusion still returns that raw value.
- Durations below binary64 epsilon return zero variance. Reversion magnitudes
  below epsilon use the zero-reversion branch. Signed reversion is supported.
- `std_deviation` is the variance square root. `evolve` adds that deviation times
  one finite standard Gaussian draw; it does not allocate a random generator.
- Invalid time/state/interval inputs and nonfinite computed results return errors.
  Existing core stabilization at extreme scales is preserved, not new binding math.

## Ownership and compatibility

C contexts and typed handles belong to their creating thread. Constructor arrays
are copied; grid outputs are caller-owned. Query outputs remain unchanged on
failure. `itofin_mf_state_process_grid` takes kind 0 times or 1 volatilities;
null output with zero capacity sizes the buffer. Short buffers preserve output
and written count. Other C query selectors are documented in the generated header.

Go serializes native calls through its Session and requires explicit Close.
Python objects are unsendable. Existing public constructors are unchanged.

## Examples and evidence

- [Python example](https://github.com/benbenbang/libitofin/blob/main/example/python/mf_state_process.py)
- [Go example](https://github.com/benbenbang/libitofin/blob/main/sdk/go/examples/mf_state_process/main.go)
- Fixture provenance: `sdk/go/testdata/mf_state_oracle.md`.

The 51 retained native observations come from actual QuantLib C++ source
`9863b578af0caa4cecabf697196533e84a8308b6`. Fifteen Decimal references separately
cover source-defined laws at extreme scales. QuantLib's separate Python1.43 wheel
does not export this process. These checks do not establish full model parity.
