# Design

[Project index](../README.md)

QuantLib is ~470k lines of mature, battle-tested C++ across 16 modules. This
project re-expresses that core in safe, idiomatic Rust:

- **Memory-safe by construction** - no manual `shared_ptr` cycles or
  use-after-free. The core is single-threaded-mutable during setup, then frozen
  into immutable snapshots for data-race-free parallel compute (`rayon`).
- **A clean FFI story** - a single core crate with Python (PyO3) and C-ABI
  (cbindgen) bindings layered on top, so the same engine is reachable from
  Python, C, C++, Julia, R, and more.
- **Faithful numerics** - QuantLib's `test-suite/` (186 `.cpp` files) is the
  porting oracle: a feature is "done" only when the matching tests are ported and
  the Rust output matches the C++ numbers within tolerance.
- **Usability at the edges** - where C++ leans on runtime casts, silent
  fallbacks, or clock magic, the core prefers compile-time typing and explicit
  errors; ergonomic conveniences live in the binding crates.

The name nods to [Kiyosi Itô](https://en.wikipedia.org/wiki/Kiyosi_It%C5%8D),
whose stochastic calculus underpins modern derivatives pricing.

## Principles

- **Bottom-up, layer by layer** - never port a module before its dependencies.
- **The C++ test-suite is the oracle** - match the numbers, not just the shape.
- **Reviewable commits** - target at most 350 changed lines, hard cap 500,
  counting additions plus deletions. Split larger work into focused commits and PRs.
- **Single-threaded-mutable core, snapshot-and-fan-out for parallelism** - the
  observable graph is mutated single-threaded during setup, then frozen into
  immutable snapshots for `rayon` compute. No `async` in the core (QuantLib does
  no I/O; market data is user input).
- **Fidelity in numerics, usability at API boundaries** - QuantLib is the oracle
  for every number, but the core favours compile-time typing and explicit `Result`
  errors over runtime casts and silent fallbacks; convenience lives in the bindings.

See [QuantLib compatibility](compatibility.md) for the detailed decisions.
