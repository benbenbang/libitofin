# Go guide

[README](../README.md) | [Python](python.md) | [Rust](rust.md) | Go

The Go SDK calls the Rust core through cgo. Use an explicit package name:

```go
import itofin "github.com/benbenbang/libitofin/sdk/go"
```

Start with [installation](../README.md#install). An external application
needs the native headers and library from the same release as its Go module;
`go get` alone does not install them. Follow the
[Go SDK setup guide](https://benbenbang.github.io/libitofin/go/#install-in-an-application)
for native packages, linker settings, and the `itofin_external` build tag.

## Examples and sessions

The [source-checkout setup](https://benbenbang.github.io/libitofin/go/#run-examples-from-a-source-checkout)
shows how to build the native library and run these examples:

- [European option](../sdk/go/examples/european_option/main.go): price a call and read its greeks, with a [walkthrough](https://benbenbang.github.io/libitofin/getting-started/).
- [Swaption example](../sdk/go/examples/swaption/main.go) (requires current `main`): price vanilla payer and Eonia OIS swaptions with checked errors, retained dependencies and live volatility repricing.
- [Portfolio simulation](../sdk/go/examples/portfolio/main.go): simulate correlated assets with a [worked explanation](https://benbenbang.github.io/libitofin/go/#portfolio-simulation).

A session owns its native object graph. Close sessions explicitly, check returned
errors, and keep objects from different sessions separate. See
[sessions and errors](https://benbenbang.github.io/libitofin/go/#sessions-and-errors)
for concurrency and callback restrictions.

## Optimization

`itofin.Minimize` runs the finance-independent Nelder-Mead solver from the
`itofin-optimize` crate on the calling goroutine, outside any session, so the
objective may call session methods such as `SimpleQuote.SetValue` and
`VanillaSwap.NPV`:

```go
result, err := itofin.Minimize(ctx, func(x []float64) (float64, error) {
	if err := rate.SetValue(x[0]); err != nil {
		return 0, err
	}
	npv, err := swap.NPV()
	return npv * npv, err
}, []float64{0.05}, itofin.NelderMeadOptions{XAtol: 1e-10})
```

- An error returned by the objective ends the run and comes back unchanged,
  so `errors.Is` works; a panic in the objective is recovered and returned as
  an error.
- Cancelling `ctx` stops the run at the end of the current iteration and
  returns the partial `OptimizeResult` with `ctx.Err()`, so
  `errors.Is(err, context.Canceled)` and `context.DeadlineExceeded` work.
- A zero `NelderMeadOptions` field keeps the solver default.
  `OptimizeStatus` values match Python `itofin.optimize.Status`.

## Reference

- [Go API reference](https://pkg.go.dev/github.com/benbenbang/libitofin/sdk/go).
- [SDK ownership and validation guide](../sdk/go/README.md).
- [Native distribution and migration](../docs/go-distribution.md).
- [C ABI contract](../docs/go-binding-contract.md) and [header](../crates/libitofin-ffi/include/itofin.h).

- [Coupled yield curves](../docs/docs/joint-curves.md): jointly fit Ibor basis markets with retained curve ownership.
