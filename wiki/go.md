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

## Reference

- [Go API reference](https://pkg.go.dev/github.com/benbenbang/libitofin/sdk/go).
- [SDK ownership and validation guide](../sdk/go/README.md).
- [Native distribution and migration](../docs/go-distribution.md).
- [C ABI contract](../docs/go-binding-contract.md) and [header](../crates/libitofin-ffi/include/itofin.h).
