# Getting started

The same pricing task in Python, Rust, and Go. All three use the Rust numerical
core. The snippets below include the real runnable files from the repository,
so the documentation stays in sync with the examples.

## Price a European option

Build a Black-Scholes market, wrap it in a vanilla option, attach the analytic engine and
read the value plus the greeks.  

=== "Python"

    ```python title="example/python/european_option.py"
    --8<-- "example/python/european_option.py"
    ```

=== "Rust"

    ```rust title="crates/libitofin/examples/european_option.rs"
    --8<-- "crates/libitofin/examples/european_option.rs"
    ```

=== "Go"

    ```go title="sdk/go/examples/european_option/main.go"
    --8<-- "sdk/go/examples/european_option/main.go"
    ```

Run them from a source checkout. For Go, first follow the
[native build and runtime setup](go.md#run-examples-from-a-source-checkout).
For an application using the published module, follow [Go installation](go.md#install-in-an-application).

=== "Python"

    ```bash
    python example/python/european_option.py
    ```

=== "Rust"

    ```bash
    cargo run --example european_option
    ```

=== "Go"

    ```sh
    cd sdk/go
    go run ./examples/european_option
    ```

The Python and Go examples price the same call: spot 60, strike 65, 90 days to expiry,
30% volatility, 8% risk-free rate, zero dividends, and Actual/360 day count.
Their expected NPV is `2.1333684449`. The Rust example illustrates the same
pricing workflow with a different market and maturity.

## More worked examples

Go examples currently cover the European option above and
[correlated portfolio simulation](go.md#portfolio-simulation). The Python and
Rust examples below cover additional products:

| Topic | Python | Rust |
|-------|--------|------|
| European option | [`european_option.py`](https://github.com/benbenbang/libitofin/blob/main/example/python/european_option.py) | [`european_option.rs`](https://github.com/benbenbang/libitofin/blob/main/crates/libitofin/examples/european_option.rs) |
| Monte Carlo | [`monte_carlo.py`](https://github.com/benbenbang/libitofin/blob/main/example/python/monte_carlo.py) | [`monte_carlo.rs`](https://github.com/benbenbang/libitofin/blob/main/crates/libitofin/examples/monte_carlo.rs) |
| Vanilla swap | [`vanilla_swap.py`](https://github.com/benbenbang/libitofin/blob/main/example/python/vanilla_swap.py) | [`vanilla_swap.rs`](https://github.com/benbenbang/libitofin/blob/main/crates/libitofin/examples/vanilla_swap.rs) |
| Yield curve | [`yield_curve.py`](https://github.com/benbenbang/libitofin/blob/main/example/python/yield_curve.py) | [`yield_curve.rs`](https://github.com/benbenbang/libitofin/blob/main/crates/libitofin/examples/yield_curve.rs) |
| Credit CDS | [`credit_cds.py`](https://github.com/benbenbang/libitofin/blob/main/example/python/credit_cds.py) | [`credit_cds.rs`](https://github.com/benbenbang/libitofin/blob/main/crates/libitofin/examples/credit_cds.rs) |
| ISDA CDS | [`isda_cds.py`](https://github.com/benbenbang/libitofin/blob/main/example/python/isda_cds.py) | [`isda_cds.rs`](https://github.com/benbenbang/libitofin/blob/main/crates/libitofin/examples/isda_cds.rs) |
| Inflation swap | [`inflation_swap.py`](https://github.com/benbenbang/libitofin/blob/main/example/python/inflation_swap.py) | [`inflation_swap.rs`](https://github.com/benbenbang/libitofin/blob/main/crates/libitofin/examples/inflation_swap.rs) |
| YoY inflation cap/floor | [`yoy_inflation_capfloor.py`](https://github.com/benbenbang/libitofin/blob/main/example/python/yoy_inflation_capfloor.py) | [`yoy_inflation_capfloor.rs`](https://github.com/benbenbang/libitofin/blob/main/crates/libitofin/examples/yoy_inflation_capfloor.rs) |
