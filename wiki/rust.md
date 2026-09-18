# Rust guide

[README](../README.md) | [Python](python.md) | Rust | [Go](go.md)

`libitofin` is the numerical core. Its modules cover dates, calendars, curves,
quotes, instruments, models, and pricing engines. Start with
[installation](../README.md#install), then the
[Rust API reference](https://docs.rs/libitofin).

## Run an example

From a source checkout, run the European option example with the repository's
[pinned Rust toolchain](../rust-toolchain.toml):

```sh
cargo run --example european_option
```

The [European option walkthrough](https://benbenbang.github.io/libitofin/getting-started/)
includes the full [runnable source](../crates/libitofin/examples/european_option.rs).
It builds a market, attaches an analytic engine, and reads the option value and
greeks.

## More examples and reference

- [Example sources](../crates/libitofin/examples/): Monte Carlo, swaps, yield curves, credit, inflation, and finite-difference option pricing.
- [Rust documentation entry point](https://benbenbang.github.io/libitofin/rust/).
- [Core source](../crates/libitofin/src/) and [tests](../crates/libitofin/tests/).
- [Project status](status.md): implementation scope and remaining work.
