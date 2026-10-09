# itofin

[`libitofin`](https://crates.io/crates/libitofin) is a ground-up port of
[QuantLib](https://www.quantlib.org/) into idiomatic Rust, with Python and Go
bindings. All three language surfaces share the Rust numerical core.

- **Fidelity in numerics, usability at the boundary.** QuantLib is the oracle for every
  number; the Python API adds ergonomic conveniences (keyword constructors, `price(engine)`)
  without changing the math.
- **Typed and introspectable.** Every class ships hand-written `.pyi` stubs with Google-style
  docstrings, so editors autocomplete and this site renders the full signature.

## Install

=== "Python"

    ```bash
    pip install itofin
    ```

=== "Rust"

    ```bash
    cargo add libitofin
    ```

=== "Go"

    Install the matching native library first, following the [Go SDK guide](go.md).
    Then add the module to your application:

    ```sh
    go get github.com/benbenbang/libitofin/sdk/go@v0.24.0
    ```

    Go requires cgo, a C compiler, and the `itofin_external` build tag for
    applications outside this repository.

The [Getting started](getting-started.md) page has runnable pricing examples
in Python, Rust, and Go. The [project index](https://github.com/benbenbang/libitofin#readme)
links to the language guides and project documentation.

## Where the docs live

| Surface | Where |
|---------|-------|
| Python API reference | This site (see the **Python API** section) |
| Rust API reference | [docs.rs/libitofin](https://docs.rs/libitofin) - see [Rust API](rust.md) |
| Go SDK | [Installation, sessions, and examples](go.md), plus [API reference](https://pkg.go.dev/github.com/benbenbang/libitofin/sdk/go) |
| Worked examples | [`example/python`](https://github.com/benbenbang/libitofin/tree/main/example/python), [`sdk/go/examples`](https://github.com/benbenbang/libitofin/tree/main/sdk/go/examples), and [`crates/libitofin/examples`](https://github.com/benbenbang/libitofin/tree/main/crates/libitofin/examples) |

## Reviewed Rust additions

- [Priority adoption scope and limits](priority-adoption.md): 18 retained High
  rows, with 10 excluded because existing issue acceptance already owns them.
- Rust-core only: no new Python, C or Go facades, and no release implied.

## Project guides

- Language guides: [Rust](https://github.com/benbenbang/libitofin/blob/main/wiki/rust.md),
  [Python](https://github.com/benbenbang/libitofin/blob/main/wiki/python.md),
  [Go](https://github.com/benbenbang/libitofin/blob/main/wiki/go.md).
- [Status and scope](https://github.com/benbenbang/libitofin/blob/main/wiki/status.md)
- [Development](https://github.com/benbenbang/libitofin/blob/main/wiki/development.md)
- [Design](https://github.com/benbenbang/libitofin/blob/main/wiki/design.md)
- [QuantLib compatibility](https://github.com/benbenbang/libitofin/blob/main/wiki/compatibility.md)

- [Small LOW Rust additions](low-small-additions.md): additive helpers, immutable
  legacy components, independent references and deferred scope.

- [Bond Z-spread analytics](bond-zspread.md): live-curve clean/dirty quotes and
  checked spread inversion with settlement and current-notional conventions.

- [Exchange-rate chaining](exchange-rate-chaining.md): ordered direct/derived
  conversions, currency orientations and checked numerical boundaries.

- [Bond forwards](bond-forward.md): retained live bonds, explicit income/financing curves and checked delivery quotes.

- [Implied volatility term structure](implied-vol-term-structure.md): fixed-reference forward variance, live source handles and explicit extrapolation.
