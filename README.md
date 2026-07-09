# Lib-Itô-Fin

[![Crates.io](https://img.shields.io/crates/v/libitofin)](https://crates.io/crates/libitofin)
[![docs.rs](https://img.shields.io/docsrs/libitofin)](https://docs.rs/libitofin)
[![License: BSD-3-Clause](https://img.shields.io/crates/l/libitofin)](LICENSE)

A ground-up port of [QuantLib](https://github.com/lballabio/QuantLib) — the
quantitative-finance library — into idiomatic, memory-safe Rust. The deliverable
is a core library, **`libitofin`**, with thin language bindings on top (Python
first, then a C ABI for everything else).

> The name nods to [Kiyosi Itô](https://en.wikipedia.org/wiki/Kiyosi_It%C5%8D),
> whose stochastic calculus underpins modern derivatives pricing.

> ⚠️ **Pre-1.0, under active development.** The core prices a European option
> end-to-end today, but the API will change until 1.0 and large parts of the
> pricing surface (processes, cashflows, most instruments and engines) are still
> being filled in. See [Status](#status).

```sh
cargo add libitofin
```

```rust
use libitofin::time::Date;
// dates, calendars, day counters, curves, vol surfaces, quotes, and an
// analytic European-option engine are available today - see docs.rs.
```

## Why

QuantLib is ~470k lines of mature, battle-tested C++ across 16 modules. This
project re-expresses that core in safe, idiomatic Rust:

- **Memory-safe by construction** — no manual `shared_ptr` cycles or
  use-after-free. The core is single-threaded-mutable during setup, then frozen
  into immutable snapshots for data-race-free parallel compute (`rayon`).
- **A clean FFI story** — a single core crate with Python (PyO3) and C-ABI
  (cbindgen) bindings layered on top, so the same engine is reachable from
  Python, C, C++, Julia, R, and more.
- **Faithful numerics** — QuantLib's `test-suite/` (186 `.cpp` files) is the
  porting oracle: a feature is "done" only when the matching tests are ported and
  the Rust output matches the C++ numbers within tolerance.
- **Usability at the edges** — where C++ leans on runtime casts, silent
  fallbacks, or clock magic, the core prefers compile-time typing and explicit
  errors; ergonomic conveniences live in the binding crates.

## Status

The port proceeds **bottom-up** through dependency layers L0→L11; each layer
depends only on lower-numbered layers. The live backlog is the
[GitHub Project board](https://github.com/users/benbenbang/projects/5) and the
repository's issues (the board is the source of truth, not a checked-in file).

| Layer | Epic | Scope | State |
|------|------|-------|-------|
| **L0** | core | types, errors, patterns, settings, handle, utilities | ✅ done |
| **L1** | math | array/matrix, distributions, interpolation, integrals, solvers, optimization, statistics, RNG, ODE, copulas, decompositions | ✅ done |
| **L2** | time | `Date`, `Period`, `Calendar`, `DayCounter`, `Schedule`, IMM/ASX/ECB | ✅ done |
| **L3** | quotes | `Quote`, `SimpleQuote`, derived quotes, `InterestRate`, compounding | ✅ done |
| **L4** | term structures | interpolated yield curves, Black-vol curves/surfaces, local vol | ✅ done |
| L5 | processes | `StochasticProcess`, Black-Scholes, Heston, … | 🚧 in progress |
| L6–L11 | indexes, cashflows, instruments, methods, models, engines | ⬜ planned |

**Milestone 1 (done):** a European option prices end-to-end — quote → flat
yield/vol curves → generalized Black-Scholes process → analytic engine → lazy
instrument greeks — matching QuantLib's `europeanoption.cpp` value and greeks to
double-rounding precision, with the full observer/invalidation graph exercised.

### What's usable today

- **`types` / `errors`** — QuantLib's numeric aliases and `QlError` / `QlResult`
  with `fail!` / `require!` macros (the analogue of `QL_FAIL` / `QL_REQUIRE`).
- **`patterns` / `handle` / `settings`** — the observer/observable graph,
  `LazyObject`, `Handle` / `RelinkableHandle`, and the evaluation-date context.
- **`math`** — arrays and matrices (with SVD/QR/Cholesky/…), the distribution
  family, interpolation (linear → bicubic), integrals (incl. Gauss quadratures),
  1-D solvers, optimizers, statistics, RNGs (MT/Sobol/…), ODEs, copulas.
- **`time`** — dates, periods, 50+ calendars, day counters, schedules, IMM/ASX/ECB.
- **`quotes` / `interestrate`** — simple and derived quotes, interest-rate and
  compounding conversions.
- **`termstructures`** — flat and interpolated yield curves (zero/discount/
  forward), implied and spreaded curves, Black-variance curves/surfaces, local vol.
- **`processes` / `instruments` / `pricingengines`** — the generalized
  Black-Scholes process, vanilla payoffs and exercise, `EuropeanOption`, and the
  analytic European engine.

## Getting started (development)

Requires the toolchain pinned in [`rust-toolchain.toml`](rust-toolchain.toml)
(Rust 1.96.0, edition 2024); plain `cargo` picks it up automatically.

```sh
cargo build                          # whole workspace
cargo build -p libitofin             # core crate only

cargo test                           # the porting oracle
cargo test -p libitofin patterns::   # one module

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
crates/libitofin/       the core library — FFI-agnostic, idiomatic Rust
crates/libitofin-ffi/   extern "C" + cbindgen → C header          (planned)
crates/libitofin-py/    PyO3 + maturin → pip-installable wheel     (planned)
QuantLib/               reference C++ tree + test oracle           (git-ignored symlink)
```

The `QuantLib/` entry is a **git-ignored local symlink**, not committed — point
it at a QuantLib checkout to have the reference source and test-suite oracle
available locally: `ln -s /path/to/QuantLib QuantLib`.

## Design principles

- **Bottom-up, layer by layer** — never port a module before its dependencies.
- **The C++ test-suite is the oracle** — match the numbers, not just the shape.
- **Small PRs** — ≤350 LOC target, 400 hard cap; large source files split across
  tickets.
- **Single-threaded-mutable core, snapshot-and-fan-out for parallelism** — the
  observable graph is mutated single-threaded during setup, then frozen into
  immutable snapshots for `rayon` compute. No `async` in the core (QuantLib does
  no I/O; market data is user input).
- **Fidelity in numerics, usability at API boundaries** — QuantLib is the oracle
  for every number, but the core favours compile-time typing and explicit `Result`
  errors over runtime casts and silent fallbacks; convenience lives in the bindings.

## Divergences from QuantLib

The port targets **semantic** faithfulness (matching QuantLib's results), not
bit-for-bit reproduction of its implementation. A small, deliberate set of
divergences is catalogued here. Each is an intentional, reviewed decision (not an
oversight) and is documented at the point of divergence in the source.

**Time / calendars (EPIC-2):**

- **Calendar holiday overrides are per-value, not process-global.** QuantLib
  shares one global `Impl` per market, so `addHoliday` on any `TARGET()` handle
  is visible through every other. This port shares added/removed holidays only
  among *clones* of a `Calendar` value, matching the "explicit state, no hidden
  singletons" decision. The built-in holiday rules are identical; only the
  reach of `add_holiday`/`remove_holiday` differs.
- **`holiday_list` filters weekends by a date-aware rule.** QuantLib's
  `holidayList` excludes weekends using the weekday-only `isWeekend`, which
  misclassifies holidays for markets whose weekend changed over time (Saudi
  Arabia's Thu/Fri→Fri/Sat in 2013, Israel/TASE's Fri/Sat→Sat/Sun in 2026). This
  port filters with a date-aware `is_weekend_on`; fixed-weekend calendars are
  unaffected (the default equals the weekday rule).
- **Table-backed calendars fail loudly past their data horizon.** Where QuantLib
  tabulates lunar / religious / observed holidays only up to a fixed year and
  then silently returns "business day" for later dates, this port panics with a
  clear message once a query passes the last fully-tabulated year. QuantLib's
  tables are kept verbatim; we never fabricate future dates.
- **`Period` comparison is a partial order, and fixes a negative-period bug.**
  QuantLib's `operator<`/`operator==` throw when two periods have overlapping
  day ranges (e.g. `1 Month` vs `30 Days`); this port returns `None` from
  `partial_cmp` instead, so comparison never panics. It also orders the day
  bounds `min <= max` before comparing, which QuantLib omits: for negative
  lengths QuantLib's inverted bounds make overlapping periods (like `-1 Month`
  vs `-30 Days`) look decidably ordered, whereas this port correctly reports
  them as undecidable. Positive comparisons are unaffected.
- **`DayCounter` is always valid; there is no empty placeholder.** QuantLib's
  default-constructed `DayCounter` holds a null `impl_` and `QL_REQUIRE`s a
  non-null one on every call. This port omits the empty state, so a
  `DayCounter` always wraps a concrete convention and its accessors never trip
  that null check. The "not yet set" placeholder used by higher layers is an
  `Option<DayCounter>` at those call sites instead.
- **`Business/252` counts directly instead of via a process-global cache.**
  QuantLib memoizes per-month and per-year business-day totals in global
  `std::map`s keyed by calendar name; this port counts with
  `Calendar::business_days_between` directly. With a calendar's built-in
  schedule the results are identical; once holidays are overridden they can
  differ, since QuantLib's name-keyed cache goes stale while this port always
  reflects the current holiday set.
- **`Actual/Actual (ISMA)` uses the reference-date algorithm.** QuantLib picks a
  schedule-driven implementation when a `Schedule` is supplied and a
  reference-date one otherwise; the reference-date path is ported, with the
  schedule-driven overload following as needed.

**Core (EPIC-0):**

- **An unset evaluation date is an explicit error, not a system-clock fallback.**
  QuantLib's `Settings` singleton falls back to the machine clock; this core has
  no clock (for determinism and FFI), so operations that need the evaluation date
  return `Err` when it is unset rather than silently pricing a possibly-expired
  instrument as live.

**Cash flows (EPIC-7):**

- **`Event::has_occurred` is a required trait method, not a defaulted one.**
  C++ gives `Event` a base implementation and lets `CashFlow` override it, so a
  cash flow on the evaluation date honours `Settings::includeTodaysCashFlows`.
  Rust has no specialization: an inherited default on the supertrait would hand
  every cash flow the plain-event rule with no diagnostic, and a competing
  provided method on `CashFlow` would be ambiguous rather than overriding. Each
  implementor therefore forwards explicitly to `event_has_occurred` or
  `cash_flow_has_occurred`, turning a silent wrong answer into a compile error.

## License

[BSD-3-Clause](LICENSE) — the same license as QuantLib, the ported source.
