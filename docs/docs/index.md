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
    go get github.com/benbenbang/libitofin/sdk/go@v0.23.0
    ```

    Go requires cgo, a C compiler, and the `itofin_external` build tag for
    applications outside this repository.

## A first taste

```python
from itofin import Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.processes import BlackScholesProcess
from itofin.time import Date

settings = Settings()
settings.set_evaluation_date(Date(15, 6, 2026))
# ... build the process, wrap it in a VanillaOption, attach an engine, read npv()
```

The full walk-through, in Python, Rust, and Go, is on the [Getting started](getting-started.md) page.

## Where the docs live

| Surface | Where |
|---------|-------|
| Python API reference | This site (see the **Python API** section) |
| Rust API reference | [docs.rs/libitofin](https://docs.rs/libitofin) - see [Rust API](rust.md) |
| Go SDK | [Installation, sessions, and examples](go.md), plus [API reference](https://pkg.go.dev/github.com/benbenbang/libitofin/sdk/go) |
| Worked examples | [`example/python`](https://github.com/benbenbang/libitofin/tree/main/example/python), [`sdk/go/examples`](https://github.com/benbenbang/libitofin/tree/main/sdk/go/examples), and [`crates/libitofin/examples`](https://github.com/benbenbang/libitofin/tree/main/crates/libitofin/examples) |
