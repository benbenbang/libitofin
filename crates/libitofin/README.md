# libitofin

A ground-up port of [QuantLib](https://github.com/lballabio/QuantLib) into idiomatic, FFI-agnostic Rust.

`libitofin` is the core quantitative-finance library: dates and calendars, day counters,
interpolation, integration, distributions, solvers, RNGs, quotes, term structures, stochastic
processes, and pricing engines. QuantLib is the correctness oracle - every ported number is matched
against its `test-suite/*.cpp` case within tolerance.

The import path is `libitofin`:

```rust
use libitofin::time::Date;
```

## Status

Early, pre-1.0, and under active development. Milestone 1 is complete: a European option prices
end-to-end (quote -> flat yield/vol curves -> Black-Scholes process -> analytic engine -> lazy
instrument greeks), matching QuantLib's `europeanoption.cpp` at double-rounding precision. Layers L4
through L11 (term structures, processes, instruments, models, engines) are being filled out.

The public API will change until 1.0. Language bindings (Python, C ABI) are planned as separate crates.

## License

[BSD-3-Clause](../../LICENSE) - the same license as QuantLib, the ported source.
