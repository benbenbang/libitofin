# Lib-Itô-Fin

[![Crates.io](https://img.shields.io/crates/v/libitofin)](https://crates.io/crates/libitofin)
[![PyPI](https://img.shields.io/pypi/v/itofin)](https://pypi.org/project/itofin/)
[![docs.rs](https://img.shields.io/docsrs/libitofin)](https://docs.rs/libitofin)
[![Python](https://img.shields.io/pypi/pyversions/itofin)](https://pypi.org/project/itofin/)
[![License: BSD-3-Clause](https://img.shields.io/crates/l/libitofin)](LICENSE)

A ground-up port of [QuantLib](https://github.com/lballabio/QuantLib) — the
quantitative-finance library — into idiomatic, memory-safe Rust. The deliverable
is a core library, **`libitofin`**, with thin language bindings on top (Python
first, then a C ABI for everything else).

> The name nods to [Kiyosi Itô](https://en.wikipedia.org/wiki/Kiyosi_It%C5%8D),
> whose stochastic calculus underpins modern derivatives pricing.

> ⚠️ **Pre-1.0, under active development.** The core already prices European
> options, Hull-White swaptions (with model calibration), and Heston options
> end-to-end, but the API will change until 1.0 and parts of the pricing surface
> are still being filled in. The **Python bindings** (`itofin`) are published on
> [PyPI](https://pypi.org/project/itofin/); C and Go bindings are available from
> this checkout. See
> [Status](#status).

## Install

```sh
cargo add libitofin
```

```rust
use libitofin::time::Date;
// dates, calendars, day counters, curves, vol surfaces, quotes, indexes,
// cashflows, swaps/swaptions, short-rate + Heston models with calibration,
// and analytic option engines are available today - see docs.rs.
```

### Python

The same engine is reachable from Python via [`itofin`](https://pypi.org/project/itofin/)
(Python 3.10+: 3.13 is the primary tested target, and 3.10-3.12 are supported via
the abi3 wheel). The API mirrors QuantLib's `ql/` layout: types live in submodules
(`itofin.time`, `itofin.instruments`, `itofin.processes`, ...), while `Settings`
and `ItofinError` stay at the top level. Real market data is first-class - yield
curves (bootstrapped from a deposit/swap strip via `PiecewiseYieldCurve`, or
interpolated), Black-vol surfaces, swaption vol cubes (matrix, interpolated, or
SABR-calibrated), and cap/floor optionlet-vol stripping - so you price against a
market snapshot, not just flat inputs. Type stubs ship in the wheel, so editors
and `mypy` see the full API.

```sh
pip install itofin
```

```python
from itofin import Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.processes import BlackScholesProcess
from itofin.time import Date, DayCounter

s = Settings()
s.set_evaluation_date(Date(15, 6, 2026))
dc = DayCounter.actual360()

process = BlackScholesProcess(60.0, 0.08, 0.0, 0.30, Date(15, 6, 2026), dc)
option = VanillaOption(OptionType.Call, 65.0, Date(15, 6, 2026) + 90, s)
option.set_engine(process)

print(f"NPV   {option.npv():.10f}")   # 2.1333684449
print(f"delta {option.delta():.10f}")  # 0.3724827980
```

…or build a real vol surface (strike x expiry) and query it:

```python
from itofin.termstructures import BlackVarianceSurface
from itofin.time import Date, DayCounter

ref = Date(15, 6, 2026)
vols = BlackVarianceSurface(
    ref,
    [ref + 365, ref + 730],   # expiries
    [90.0, 100.0, 110.0],     # strikes
    [[0.20, 0.25],            # one row per strike,
     [0.18, 0.22],            # one column per expiry
     [0.16, 0.20]],
    DayCounter.actual365_fixed(),
)
print(f"{vols.black_vol(1.0, 100.0):.2f}")  # 0.18
```

…or query a swaption vol surface - here the ATM matrix; `InterpolatedSwaptionVolatilityCube`
and the SABR-calibrated `SabrSwaptionVolatilityCube` add the strike dimension, and
`pricingengines.BlackSwaptionEngine` (via `Swaption.set_black_engine`) prices a
swaption straight off any of them:

```python
from itofin import Settings
from itofin.termstructures import SwaptionVolatilityMatrix, VolatilityType
from itofin.time import Date, Period, Calendar, DayCounter, BusinessDayConvention

s = Settings()
s.set_evaluation_date(Date(15, 6, 2026))
opt = [Period(1, "Years"), Period(5, "Years")]   # option tenors (rows)
swp = [Period(1, "Years"), Period(5, "Years")]   # swap tenors (columns)
vols = [[0.20, 0.18],
        [0.17, 0.16]]

svol = SwaptionVolatilityMatrix(
    Date(15, 6, 2026), Calendar.target(), BusinessDayConvention.Following,
    opt, swp, vols, DayCounter.actual365_fixed(), VolatilityType.ShiftedLognormal,
)
print(f"{svol.volatility(Period(1, 'Years'), Period(5, 'Years'), 0.03):.4f}")  # 0.1800 (node)
print(f"{svol.volatility(Period(3, 'Years'), Period(3, 'Years'), 0.03):.4f}")  # 0.1775 (bilinear)
```

### Go and C

The [Go package](sdk/go/README.md) calls the same Rust core through the
[C ABI](crates/libitofin-ffi/include/itofin.h). Build it with Rust 1.96.0,
Go 1.27.1, and a C compiler. Each Go session confines its mutable native graph
to one OS thread; independent sessions can run concurrently. Close sessions
explicitly. A native panic poisons its session, which must then be closed.

The Go module is `github.com/benbenbang/libitofin/sdk/go`. Install a version with
a matching `sdk/go/vVERSION` tag and native release package. Existing v0.22.0
consumers retain the published `bindings/go` path.
See the [installation and migration guide](docs/go-distribution.md).

Start with the [build and ownership guide](sdk/go/README.md) and the
[synthetic portfolio example](sdk/go/examples/portfolio/main.go).
The [binding contract](docs/go-binding-contract.md) specifies the boundary;
[tracker #1000](https://github.com/benbenbang/libitofin/issues/1000) records
remaining parity and delivery work. API mappings, numerical tests, and statement
coverage are separate measures; see the [current validation record](docs/go-bindings-followups.md).

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
| **L4** | term structures | interpolated yield curves, Black-vol curves/surfaces, local vol, swaption vol surfaces (matrix / interpolated / SABR cube), cap-floor term-vol surfaces + optionlet stripping | ✅ done |
| **L5** | processes | multi-factor `StochasticProcess` / `StochasticProcess1D`, Black-Scholes, Heston (analytic surface), Ornstein-Uhlenbeck, correlated process array | 🚧 in progress |
| **L6** | indexes | `InterestRateIndex`, Ibor family (Euribor / Eonia / €STR / SOFR), `SwapIndex` | 🚧 in progress |
| **L7** | cashflows | fixed / floating / Ibor / overnight coupons and legs, coupon pricers, duration, capped-floored coupons | 🚧 in progress |
| **L8** | instruments | fixed-rate bonds, vanilla / OIS swaps, swaptions, caps & floors, vanilla options | 🚧 in progress |
| **L9** | methods | lattices, trees (trinomial + Hull-White), Monte Carlo (path generators + antithetic), finite differences (European, American and Bermudan Black-Scholes vanilla) | 🚧 in progress |
| **L10** | models | `CalibratedModel` + `calibrate()`, short-rate (Vasicek, CIR, Hull-White), Heston, calibration helpers | 🚧 in progress |
| **L11** | engines | analytic European & Heston (Fourier), swaption (Black / Bachelier / Jamshidian), discounting swap / bond, Black cap/floor | 🚧 in progress |

**Milestone 1 (done):** a European option prices end-to-end — quote → flat
yield/vol curves → generalized Black-Scholes process → analytic engine → lazy
instrument greeks — matching QuantLib's `europeanoption.cpp` value and greeks to
double-rounding precision, with the full observer/invalidation graph exercised.

Verified end-to-end since then, each against the matching `test-suite/` oracle:

- **Hull-White calibration** — a Hull-White model calibrates to a swaption strip
  (Jamshidian decomposition + Levenberg-Marquardt), reproducing
  `shortratemodels.cpp`'s cached mean-reversion and volatility.
- **Heston analytic + calibration** — the Fourier `AnalyticHestonEngine` prices
  European options to `hestonmodel.cpp`'s cached values, and a Heston model
  calibrates to a DAX volatility surface (reproducing the reference SSE).

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
  forward), implied and spreaded curves, Black-variance curves/surfaces, local vol,
  swaption vol surfaces (matrix / interpolated / SABR cube) and cap-floor term-vol
  surfaces with optionlet stripping (SABR smile sections + calibrated interpolation).
- **`indexes`** — `InterestRateIndex`, the Ibor family (Euribor / Eonia / €STR /
  SOFR) and `SwapIndex`, with fixings threaded through `Settings` (D11).
- **`cashflows`** — fixed, floating, Ibor and overnight coupons and legs, coupon
  pricers, duration, and capped-floored coupons.
- **`processes`** — the multi-factor `StochasticProcess` base, generalized
  Black-Scholes, Heston (analytic surface), Ornstein-Uhlenbeck, and a correlated
  process array.
- **`models`** — `CalibratedModel` with `calibrate()`, the short-rate family
  (Vasicek, CIR, Hull-White), the Heston model, and calibration helpers
  (swaption, Heston).
- **`instruments` / `pricingengines`** — vanilla payoffs and exercise,
  `EuropeanOption`, fixed-rate bonds, vanilla / OIS swaps, swaptions, caps &
  floors; the analytic European and Heston (Fourier) engines, the swaption
  engines (Black / Bachelier / Jamshidian), and discounting swap / bond engines.

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
crates/libitofin-ffi/   extern "C" + cbindgen -> C header + libitofin_ffi
crates/itofin-py/       PyO3 + maturin -> the `itofin` package       (on PyPI)
sdk/go/                Go 1.27.1 cgo package with explicit sessions
sdk/node/              Node SDK placeholder (not implemented)
QuantLib/               reference C++ tree + test oracle           (git-ignored symlink)
```

The `QuantLib/` entry is a **git-ignored local symlink**, not committed — point
it at a QuantLib checkout to have the reference source and test-suite oracle
available locally: `ln -s /path/to/QuantLib QuantLib`.

## Design principles

- **Bottom-up, layer by layer** — never port a module before its dependencies.
- **The C++ test-suite is the oracle** — match the numbers, not just the shape.
- **Reviewable commits** - target at most 350 changed lines, hard cap 500,
  counting additions plus deletions. Split larger work into focused commits and PRs.
- **Single-threaded-mutable core, snapshot-and-fan-out for parallelism** — the
  observable graph is mutated single-threaded during setup, then frozen into
  immutable snapshots for `rayon` compute. No `async` in the core (QuantLib does
  no I/O; market data is user input).
- **Fidelity in numerics, usability at API boundaries** — QuantLib is the oracle
  for every number, but the core favours compile-time typing and explicit `Result`
  errors over runtime casts and silent fallbacks; convenience lives in the bindings.

## Divergences from QuantLib

See the [QuantLib compatibility notes](wiki/compatibility.md).

## License

[BSD-3-Clause](LICENSE) — the same license as QuantLib, the ported source.
