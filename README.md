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

- **Memory-safe by construction** — no manual `shared_ptr` cycles or
  use-after-free. The core is single-threaded-mutable during setup, then frozen
  into immutable snapshots for data-race-free parallel compute (`rayon`); see
  the concurrency model (D6) in [`TICKETS.md`](TICKETS.md).
- **A clean FFI story** — a single core crate with Python (PyO3) and C-ABI
  (cbindgen) bindings layered on top, so the same engine is reachable from
  Python, C, C++, Julia, R, and more.
- **Faithful numerics** — QuantLib's `test-suite/` (186 `.cpp` files) is the
  porting oracle: a feature is "done" only when the matching tests are ported
  and the Rust output matches the C++ numbers within tolerance.

## Status

The port proceeds **bottom-up** through dependency layers L0→L11; each layer
depends only on lower-numbered layers (L1 builds on L0, L2 on L0–L1, and so on).
The full backlog lives in [`TICKETS.md`](TICKETS.md).

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
crates/itofin-ffi/    extern "C" + cbindgen → C header          (planned)
crates/itofin-py/     PyO3 + maturin → pip-installable wheel     (planned)
TICKETS.md            dependency-ordered porting backlog + design decisions D1–D8
QuantLib/             reference C++ tree + test oracle           (git-ignored)
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
- **Cross-cutting decisions are settled before the code that depends on them** —
  see **D1–D8** in [`TICKETS.md`](TICKETS.md) (`Rc` vs `Arc`, error handling,
  concurrency, bindings, logging, …).

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
  singletons" decision (D5). The built-in holiday rules are identical; only the
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
  clear message once a query passes the last fully-tabulated year (the
  *minimum* across a calendar's required holiday tables). QuantLib's tables are
  kept verbatim; we never fabricate future dates.
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
  that null check. (Individual conventions may still panic on their own
  preconditions - the Canadian and ISMA counters require a valid reference
  period - so a call is not unconditionally infallible.) The "not yet set"
  placeholder used by higher layers (schedules, coupons) will be an
  `Option<DayCounter>` at those call sites when they are ported.
- **`Business/252` counts directly instead of via a process-global cache.**
  QuantLib memoizes per-month and per-year business-day totals in global
  `std::map`s keyed by calendar name. That hidden mutable state conflicts with
  the "explicit state, no singletons" decision (D5), so this port counts with
  `Calendar::business_days_between` directly. With a calendar's built-in
  schedule the results are identical (QuantLib's month/year decomposition is a
  pure caching optimization whose segments telescope to a single count); once
  holidays are overridden they can differ, since QuantLib's name-keyed cache is
  populated once and goes stale while this port always reflects the current
  holiday set.
- **`Actual/Actual (ISMA)` uses the reference-date algorithm only.** QuantLib
  picks a schedule-driven implementation when a `Schedule` is supplied and a
  reference-date one otherwise; only the reference-date path is ported, since
  `Schedule` is not yet available. The schedule-driven overload will follow when
  `Schedule` lands.

## License

This is a port of QuantLib, which is distributed under a modified BSD license.
Licensing for this project is to be finalized — see the repository owner.
