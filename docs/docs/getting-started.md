# Getting started

The same pricing task in Python and in Rust. The Python API is the thin binding; the Rust
core is where the numerics live. Both snippets below are the real, runnable example files
from the repository (`example/python` and `example/rust`), included verbatim, so what you
read here is exactly what runs.

## Price a European option

Build a Black-Scholes market, wrap it in a vanilla option, attach the analytic engine and
read the value plus the greeks.  

=== "Python"

    ```python title="example/python/european_option.py"
    --8<-- "example/python/european_option.py"
    ```

=== "Rust"

    ```rust title="example/rust/european_option.rs"
    --8<-- "example/rust/european_option.rs"
    ```

Run them with:

=== "Python"

    ```bash
    python example/python/european_option.py
    ```

=== "Rust"

    ```bash
    cargo run --example european_option
    ```

## More worked examples

Every example ships in both languages with a matching filename. Browse the full set:

| Topic | Python | Rust |
|-------|--------|------|
| European option | [`european_option.py`](https://github.com/benbenbang/libitofin/blob/main/example/python/european_option.py) | [`european_option.rs`](https://github.com/benbenbang/libitofin/blob/main/example/rust/european_option.rs) |
| Monte Carlo | [`monte_carlo.py`](https://github.com/benbenbang/libitofin/blob/main/example/python/monte_carlo.py) | [`monte_carlo.rs`](https://github.com/benbenbang/libitofin/blob/main/example/rust/monte_carlo.rs) |
| Vanilla swap | [`vanilla_swap.py`](https://github.com/benbenbang/libitofin/blob/main/example/python/vanilla_swap.py) | [`vanilla_swap.rs`](https://github.com/benbenbang/libitofin/blob/main/example/rust/vanilla_swap.rs) |
| Yield curve | [`yield_curve.py`](https://github.com/benbenbang/libitofin/blob/main/example/python/yield_curve.py) | [`yield_curve.rs`](https://github.com/benbenbang/libitofin/blob/main/example/rust/yield_curve.rs) |
| Credit CDS | [`credit_cds.py`](https://github.com/benbenbang/libitofin/blob/main/example/python/credit_cds.py) | [`credit_cds.rs`](https://github.com/benbenbang/libitofin/blob/main/example/rust/credit_cds.rs) |
| ISDA CDS | [`isda_cds.py`](https://github.com/benbenbang/libitofin/blob/main/example/python/isda_cds.py) | [`isda_cds.rs`](https://github.com/benbenbang/libitofin/blob/main/example/rust/isda_cds.rs) |
| Inflation swap | [`inflation_swap.py`](https://github.com/benbenbang/libitofin/blob/main/example/python/inflation_swap.py) | [`inflation_swap.rs`](https://github.com/benbenbang/libitofin/blob/main/example/rust/inflation_swap.rs) |
| YoY inflation cap/floor | [`yoy_inflation_capfloor.py`](https://github.com/benbenbang/libitofin/blob/main/example/python/yoy_inflation_capfloor.py) | [`yoy_inflation_capfloor.rs`](https://github.com/benbenbang/libitofin/blob/main/example/rust/yoy_inflation_capfloor.rs) |
