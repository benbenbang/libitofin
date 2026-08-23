# itofin

`itofin` is the Python binding for [`libitofin`](https://crates.io/crates/libitofin), a
ground-up port of [QuantLib](https://www.quantlib.org/) into idiomatic Rust. The Rust core
does the numerics; the Python package is a thin, typed surface on top.

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

The full walk-through, in both languages, is on the [Getting started](getting-started.md) page.

## Where the docs live

| Surface | Where |
|---------|-------|
| Python API reference | This site (see the **Python API** section) |
| Rust API reference | [docs.rs/libitofin](https://docs.rs/libitofin) - see [Rust API](rust.md) |
| Worked examples | [`example/python`](https://github.com/benbenbang/ito-fin/tree/main/example/python) and [`example/rust`](https://github.com/benbenbang/ito-fin/tree/main/example/rust) |
