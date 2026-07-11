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
- **`CashFlow::ex_coupon_date` is a required trait method.**
  C++ answers both `Coupon::accruedPeriod`'s ex-coupon test and
  `CashFlow::exCouponDate()` from one member (`coupon.hpp:57`). Rust has no
  specialization, so a `Coupon` cannot override a provided method on its
  `CashFlow` supertrait, and defaulting that method to `None` would let an
  implementor accrue ex-coupon while reporting `trading_ex_coupon` as `false`.
  It is therefore required, so omission is a compile error. The single reader is
  preserved: `Coupon::trades_ex_coupon_on` goes through `ex_coupon_date`, which
  a concrete coupon forwards to its `CouponBase`, and both `accrued_period` and
  `accrued_amount` test the ex-coupon branch through that one helper.
- **`CashFlow::as_coupon` is a required trait method.**
  The `isCoupon`/`coupon_cast` pair (`cashflow.hpp:79`, `coupon.hpp:96`), which
  the `CashFlows` analytics use to tell an accruing flow from a plain payment.
  C++ needs no default here: its `Coupon` base class overrides `isCoupon()` to
  `true`, so no coupon can forget. A Rust `Coupon` is a trait and cannot supply
  its supertrait's method on its implementors' behalf, which is the same
  no-specialization argument as `ex_coupon_date` above. A coupon answering
  `None` contributes nothing to `bps`, has its amount subtracted from
  `atm_rate`'s target, and reports its payment date to `maturity_date` in place
  of its accrual end: beside coupons that answer correctly this skews the rate,
  and alone in a leg it drives the rate to `0.0`. The method is therefore
  required, so omission is a compile error. A deliberate `None` from a `Coupon`
  remains possible, and the analytics cannot detect it. A blanket
  `impl<T: Coupon> CashFlow for T` would make that lie a compile error too, at
  the cost of moving every `CashFlow` body a coupon overrides onto `Coupon`;
  it is tracked separately.

**Pricing engines (Milestone 1):**

- **Zero-volatility Black greeks dispatch on the stored option type, not on the
  sign of `alpha_`.** This is a deliberate divergence where the oracle is
  wrong, and the only place in the port where a *finite* priced number
  intentionally disagrees with QuantLib. Upstream (tree `v1.42.1-266-g9863b578a`,
  from commit `17f1a1bed` "Fixing zero vol for Black") detects the option type
  in its zero-vol branches with `if (alpha_ >= 0) // Call`, at nine sites
  (`blackcalculator.cpp:215,222,229,257,264,271,439,446,453`). For a
  plain-vanilla put, `alpha_ = -1.0 + cum_d1_` (`blackcalculator.cpp:137`), and
  at zero volatility an out-of-the-money put has `cum_d1_ == 1.0` exactly, so
  `alpha_ == 0.0` exactly and `0.0 >= 0` takes the Call branch: the OTM put is
  handed a delta of `+1.0` where the correct value is `0.0`. This port instead
  dispatches on the option type stored in the calculator, implementing the
  values the reference's own comments state, so the OTM-put delta is `0.0`. The
  full zero-vol ladder is pinned by `zero_volatility_ladder_matches_stated_intent`
  and `zero_volatility_otm_put_gets_put_greeks_not_call_greeks` in
  `blackcalculator.rs`, which assert the corrected numbers (OTM `0.0`, ATM
  `-0.5`, ITM `-1.0` for the put, and the call mirror), so a regression to the
  `alpha_ >= 0` form is a test failure.

**Non-finite inputs (cross-cutting):**

- **Non-finite arguments are rejected at the API boundary.** QuantLib validates
  signs (`stdDev >= 0`, `forward > 0`, `t >= 0`) and relies on NaN failing every
  such comparison, so a NaN is already an error wherever a sign is checked. An
  infinity is not: it passes `>= 0.0` and propagates to a NaN result several
  layers down, and where a curve extrapolates the range check is skipped
  entirely. Following D10, this port widens each of those guards from "not NaN"
  to "finite", and adds finiteness checks where C++ has none at all: solver
  arguments and functor values (`solver1d.rs`), sampled quadrature abscissae
  (`discrete.rs`), Black-Scholes process arguments (`blackscholesprocess.rs`),
  and the volatility and variance an implementation returns
  (`termstructures/volatility/`). Each site names the C++ guard it extends, or
  states that none exists. The only behavioural change is for infinities; every
  finite input QuantLib accepts is still accepted, so no priced number moves.
- **Statistics accumulators reject a NaN sample value, and accept infinities.**
  QuantLib's only sample guard is `QL_REQUIRE(weight >= 0.0)`
  (`generalstatistics.hpp:233`, `incrementalstatistics.cpp:127`), which this
  port keeps verbatim, written `!(weight >= 0.0)` so a NaN weight fails it as it
  does in C++. A NaN *value* has no C++ guard: it is accumulated and poisons
  every subsequent mean, variance and percentile with no diagnostic. Infinite
  values remain accepted, as in C++, being meaningful to `min`, `max` and the
  risk measures.
- **Shape mismatches panic with a named cause.** `SVD::solveFor`
  (`svd.cpp:528`) and the default `CostFunction::gradient` / `jacobian` have no
  `QL_REQUIRE`; a wrongly-sized output leaves stale entries the optimiser reads
  as real derivatives. These are caller errors, not market-data errors, so the
  port asserts rather than returning `Err`.

## License

[BSD-3-Clause](LICENSE) — the same license as QuantLib, the ported source.
