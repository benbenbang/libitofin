# Lib-Itô-Fin

A ground-up port of [QuantLib](https://github.com/lballabio/QuantLib) — the
quantitative-finance library — into idiomatic Rust. The deliverable is a core
library, **`libitofin`**, with thin language bindings on top (Python first, then
a C ABI for everything else).

> The name nods to [Kiyosi Itô](https://en.wikipedia.org/wiki/Kiyosi_It%C5%8D),
> whose stochastic calculus underpins modern derivatives pricing.

> ⚠️ **Early days.** The foundational layer (EPIC-0) is in place and the
> architecture is being de-risked before the library goes wide. It is not yet
> usable for pricing. See [Status](#status).

## Why

QuantLib is ~470k lines of mature, battle-tested C++ across 16 modules. This
project re-expresses that core in safe, idiomatic Rust:

- **Memory-safe and thread-safe by construction** — no manual `shared_ptr`
  cycles, no data races on the compute path.
- **A clean FFI story** — a single core crate with Python (PyO3) and C-ABI
  (cbindgen) bindings layered on top, so the same engine is reachable from
  Python, C, C++, Julia, R, and more.
- **Faithful numerics** — QuantLib's `test-suite/` (186 `.cpp` files) is the
  porting oracle: a feature is "done" only when the matching tests are ported
  and the Rust output matches the C++ numbers within tolerance.

## Status

The port proceeds **bottom-up** through dependency layers L0→L11; each layer
depends only on the ones above it. The full backlog lives in
[`TICKETS.md`](TICKETS.md).

| Layer | Epic | Scope | State |
|------|------|-------|-------|
| **L0** | EPIC-0 core | types, errors, patterns, settings, handle, utilities | ✅ landed |
| L1 | EPIC-1 math | array/matrix, distributions, interpolation, solvers, RNG, … | 🚧 starting |
| L2 | EPIC-2 time | `Date`, `Period`, `Calendar`, `DayCounter`, `Schedule` | ⬜ |
| L3 | EPIC-3 quotes | `Quote`, `InterestRate`, compounding | ⬜ |
| L4–L11 | term structures, processes, instruments, pricing engines, models, Monte Carlo | ⬜ |

**Next milestone:** a vertical slice — price a European option end-to-end against
`europeanoption.cpp` — to validate the architecture before scaling out.

### What EPIC-0 provides today

The core crate currently builds `libitofin` with the foundational machinery the
rest of the port hangs off of (44 unit tests, all green):

- **`types`** — QuantLib's numeric aliases (`Real`, `Integer`, `Size`, `Rate`,
  `Time`, `DiscountFactor`, `Volatility`, …).
- **`errors`** — `QlError` / `QlResult` with the `fail!`, `require!`,
  `assert_ql!`, `ensure!` macros (the Rust analogue of `QL_FAIL` / `QL_REQUIRE`).
- **`patterns`** — the observer/observable graph, `LazyObject` (calculate-on-
  demand with caching), and the visitor pattern.
- **`handle`** — `Handle<T>` / `RelinkableHandle<T>`, the shared relinkable
  pointer that propagates changes to every copy.
- **`settings`** — the evaluation-date / pricing-flag context (an explicit value
  object, not QuantLib's global singleton).
- **`utilities`** — null sentinels, output formatters, a stepping iterator, and
  a deep-copy `ValueBox<T>`.

## Getting started

Requires the toolchain pinned in [`rust-toolchain.toml`](rust-toolchain.toml)
(Rust 1.96.0, edition 2024); plain `cargo` picks it up automatically.

```sh
# build the workspace (or just the core crate)
cargo build
cargo build -p itofin

# run the tests (the porting oracle)
cargo test
cargo test -p itofin                 # core crate only
cargo test -p itofin patterns::      # one module

# format & lint
cargo fmt
cargo clippy --all-targets
```

A [`pre-commit`](https://pre-commit.com/) config runs `fmt`, `check`, `clippy`,
`test`, and conventional-commit linting on every commit:

```sh
pre-commit run --all-files
```

## Project layout

```
crates/itofin/        the core library (libitofin) — FFI-agnostic, idiomatic Rust
crates/itofin-ffi/    extern "C" + cbindgen → C header   (planned)
crates/itofin-py/     PyO3 + maturin → pip-installable wheel (planned)
QuantLib/             symlink to the C++ source tree — the reference / oracle
TICKETS.md            dependency-ordered porting backlog + design decisions D1–D8
CLAUDE.md             contributor guidelines
```

## Design principles

- **Bottom-up, layer by layer** — never port a module before its dependencies.
- **The C++ test-suite is the oracle** — match the numbers, not just the shape.
- **Small PRs** — ≤350 LOC target, 400 hard cap; large source files split across
  tickets.
- **Single-threaded-mutable core, snapshot-and-fan-out for parallelism** — the
  observable graph is mutated single-threaded during setup, then frozen into
  immutable snapshots for `rayon` compute. No `async` in the core (QuantLib does
  no I/O; market data is user input).
- **Cross-cutting decisions are settled before the code that depends on them** —
  see **D1–D8** in [`TICKETS.md`](TICKETS.md) (`Rc` vs `Arc`, error handling,
  concurrency, bindings, logging, …).

## License

This is a port of QuantLib, which is distributed under a modified BSD license.
Licensing for this project is to be finalized — see the repository owner.
