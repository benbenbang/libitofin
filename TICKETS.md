# ito-fin - QuantLib → Rust Porting Backlog

A ticketized plan for translating [QuantLib](https://github.com/lballabio/QuantLib) (`ql/`, ~470k LOC,
16 modules) into idiomatic Rust. Port **bottom-up**: each layer depends only on layers above it.

The QuantLib `test-suite/` (186 `.cpp` files) is the **porting oracle** - every ticket is "done" when
the matching test cases are ported and the Rust output matches the C++ numbers within tolerance.

> Source of truth for the C++ tree: the `QuantLib/` symlink at repo root.

---

## 0. Cross-cutting design decisions (DECIDE FIRST - blocks all porting)

These five C++ idioms appear in every module; their Rust mapping defines how every other ticket is
written. Proposed defaults below are **single-threaded first** - revisit `Arc`/threading after Milestone 1.

| # | QuantLib idiom | Proposed Rust mapping | Status |
|---|---|---|---|
| D1 | `Observable` / `Observer` (push notifications) | `Observable` trait + weak-ref observer registry; dirty-flag propagation | ⬜ needs sign-off |
| D2 | `Handle<T>` / `RelinkableHandle<T>` | `Handle<T>` newtype over `Rc<RefCell<Link<T>>>` | ⬜ needs sign-off |
| D3 | `ext::shared_ptr<T>` (pervasive) | `Rc<T>` now; `Arc<T>` only if/when threaded | ⬜ needs sign-off |
| D4 | `QL_REQUIRE` / `QL_FAIL` (exceptions) | `Result<T, QlError>` with `thiserror` | ⬜ needs sign-off |
| D5 | `Settings` singleton (global eval date) | explicit `&Context` (NOT `thread_local` - invisible to compute threads, see D6) | ⬜ needs sign-off |
| D6 | runtime concurrency model | `rayon` **snapshot-and-fan-out**; no `async` in the core | ⬜ needs sign-off |
| D7 | language bindings | PyO3 + maturin (Python); `extern "C"` + cbindgen (C ABI) in sibling crates | ⬜ needs sign-off |
| D8 | logging / observability | `log` facade (zero-cost without a subscriber), coarse boundaries only - **deferred, non-blocking** | ⬜ deferred |

**D1–D5 are the QL-0.x foundation tickets - see Epic L0. D6/D7 shape the core API and couple back to D3. D8 is deferred - it touches no existing ticket and can land any time.**

### D6 - Concurrency model (`rayon` only; the core does no IO)

QuantLib fetches no market data - quotes/curves are user input - so the core has **no `async`/`tokio` story**.
`async` belongs only to an optional service built *on top* of `libitofin`, which is out of scope here.

Runtime parallelism is CPU-bound → **`rayon` threads**, using **snapshot-and-fan-out**:

1. *Setup/calibration* - single-threaded; mutate the observable graph (D1/D2), warm `LazyObject` caches.
2. *Compute* - freeze an **immutable snapshot** of inputs, then `par_iter` over it with a shared `&Snapshot`
   (`Sync` because immutable - no locks on the hot path).

High-value parallel features (land in L9–L11, but the pattern is a core constraint now): Monte Carlo (per-path),
greeks/scenario revaluation (per-scenario), portfolio pricing (per-instrument), calibration (per-basket-instrument).
Not parallel: within-curve bootstrap (iterative), FD time-stepping (serial).

### D7 - Binding strategy (couples to D3)

Core stays idiomatic and FFI-agnostic; binding crates adapt at the edge:

```
crates/itofin       core (libitofin) - knows nothing about FFI
crates/itofin-ffi   extern "C" + cbindgen → C header (lingua franca for C/C++/Julia/R/C#/Java)
crates/itofin-py    PyO3 + maturin → pip-installable wheel
```

⚠️ **D3 coupling:** PyO3 `#[pyclass]` wants `Send` by default. An `Rc<RefCell>` observable graph forces
`#[pyclass(unsendable)]` (panics if touched from another Python thread). Wanting thread-safe Python objects
(and combining with D6 `rayon`) pushes D3 toward `Arc`. **Decide D3 with bindings + D6 in mind.**

FFI shape constraints (design the core API around these, don't compromise the core):
- generics/trait objects don't cross FFI → expose concrete/enum facades in the binding layer, keep the core generic.
- ownership across the boundary → opaque handles; PyO3 manages it, the raw C ABI needs explicit `free` fns.

### D8 - Logging / observability (deferred; does not block any ticket)

QuantLib has no logging - it's a numeric library, not a service. The core stays **dep-free and IO-free**, so
logging is opt-in and should impose **minimal overhead when disabled**:

- Use the **`log` facade** (not `tracing`): a `log::debug!` with no installed logger is a near-noop and
  avoids formatting work. `tracing`'s spans/subscribers are heavier and belong to a service layer, not the core.
- **Coarse boundaries only.** Log at calibration/bootstrap entry-exit or solver non-convergence - *never* inside
  hot paths like `Observable::notify_observers`, relinks, or per-path/per-scenario loops (D6), which would emit
  millions of lines and distort timing.
- Bindings choose the sink: the PyO3 crate (D7) can bridge `log` → Python `logging`; the C ABI exposes a callback.

**Status: deferred.** No existing ticket depends on it; it can be added in any later PR without reworking the
core. Revisit when a real diagnostic need appears (e.g. debugging curve bootstrap non-convergence in L4).

---

## 1. Epic dependency map

Port order is top-to-bottom. `experimental/` is excluded (port last or never).

| Layer | Epic | Modules | ~LOC | Depends on |
|---|---|---|---|---|
| L0 | **EPIC-0 core** | `types`, `errors`, `patterns/`, `settings`, `handle`, `utilities/` | 5k | - |
| L1 | **EPIC-1 math** | array/matrix, distributions, interpolations, integrals, solvers, optimization, statistics, rng, ode | 34k | L0 |
| L2 | **EPIC-2 time** | `Date`, `Period`, `Calendar`, `DayCounter`, `Schedule` | 20k | L0 |
| L3 | **EPIC-3 quotes** | `Quote`, `SimpleQuote`, `interestrate`, `compounding` | 1.5k | L0, L2 |
| L4 | **EPIC-4 termstructures** | yield, volatility, credit, inflation | 34k | L1, L2, L3 |
| L5 | **EPIC-5 processes** | `stochasticprocess`, `processes/` | 6k | L1, L4 |
| L6 | **EPIC-6 indexes** | IBOR, swap, inflation indices | 7k | L2, L4 |
| L7 | **EPIC-7 cashflows** | coupons, legs, cashflow vectors | 15k | L4, L6 |
| L8 | **EPIC-8 instruments** | `instrument`, `exercise`, `payoff`, options, bonds, swaps | 23k | L5, L7 |
| L9 | **EPIC-9 methods** | montecarlo, lattices, finitedifferences | 21k | L5 |
| L10 | **EPIC-10 models** | shortrate, equity (Heston), marketmodels | 33k | L5, L9 |
| L11 | **EPIC-11 engines** | vanilla, barrier, asian, swaption, ... | 46k | L8, L9, L10 |
| L12 | ~~experimental~~ | - | 66k | EXCLUDED |

---

## 2. Ticket convention

Modules are far larger than one PR, so the hierarchy is **Epic → Ticket → PR**, and PRs stay **≤350 LOC**
(400 hard cap) per project guidelines. Any source file over that is split into multiple tickets
(struct/ctors → methods → tests).

```
[QL-<epic>.<n>] <component>
 Scope:       <single .hpp/.cpp pair or one coherent subdir>
 Depends on:  [QL-x.y, ...]
 Port:        ql/<path>  ->  crate::<module>
 Acceptance:  port test-suite/<name>.cpp cases; match C++ within <tol>
 Size:        S (<100 LOC) | M (100-350) | L (must be split)
```

---

## 3. Milestone 1 - Vertical slice (do this BEFORE going wide)

**Goal: price one European option under Black-Scholes-Merton, end-to-end, matching `europeanoption.cpp`.**

This proves every cross-cutting decision (D1–D5) works against real code before committing to the full
backlog. It cuts a thin slice through L0 → L1 → L2 → L3 → L4(flat curve) → L8(payoff/exercise) → L11(analytic engine):

`QL-0.1, QL-0.2, QL-0.3, QL-0.5, QL-0.6` · `QL-2.1, QL-2.2` · `QL-1.3` · `QL-3.1, QL-3.2` ·
flat yield/vol term structures · `Payoff`+`Exercise`+`VanillaOption` · `AnalyticEuropeanEngine`.

Acceptance: `europeanoption.cpp` price + greeks match within 1e-10.

---

## 4. EPIC-0 - core (L0)  ✳️ start here

| ID | Component | Port | Acceptance | Size |
|---|---|---|---|---|
| QL-0.1 | Types | `ql/types.hpp` → `crate::types` (Real, Size, Time, Rate, Spread, Volatility, …) | compiles; alias smoke tests | S |
| QL-0.2 | Errors | `ql/errors.{hpp,cpp}` → `QlError` + `require!`/`fail!` macros (D4) | error paths return `Err` | S |
| QL-0.3 | Observable/Observer | `ql/patterns/observable.{hpp,cpp}` (D1) | notify → observers marked dirty | M |
| QL-0.4 | LazyObject + Singleton + Visitor + CRTP | `ql/patterns/{lazyobject,singleton,visitor,curiouslyrecurring}.hpp` | lazy recalc fires once per notify | M |
| QL-0.5 | Handle / RelinkableHandle | `ql/handle.hpp` (D2) | relink propagates notification | M |
| QL-0.6 | Settings | `ql/settings.{hpp,cpp}` (D5) | global eval date get/set | S |
| QL-0.7 | Utilities | `ql/utilities/` (null, dataformatters, steppingiterator, clone_ptr) | unit tests per helper | M |

---

## 5. EPIC-2 - time (L2)  ✳️ parallelizable with EPIC-1

| ID | Component | Port | Acceptance | Size |
|---|---|---|---|---|
| QL-2.1 | Date + Period | `ql/time/date.*`, `ql/time/period.*` | `dates.cpp`, `period.cpp` | L → split (Date core / arithmetic / Period) |
| QL-2.2 | DayCounters | `ql/time/daycounters/` (Actual360/365, 30/360, Thirty360, ActualActual, Business252) | `daycounters.cpp` | L → 1 ticket per convention |
| QL-2.3 | Calendar base + Weekend logic | `ql/time/calendar.*`, business-day adjustment, `joinHolidays` | adjust/advance on a stub calendar | M |
| QL-2.4 | Schedule | `ql/time/schedule.*` | `schedule.cpp` | L → split (DateGeneration rules / Schedule build) |
| QL-2.5+ | Per-country calendars | `ql/time/calendars/*` (UnitedStates, TARGET, UnitedKingdom, …) | relevant cases in `calendars.cpp` | S each (one ticket per calendar) |

---

## 6. EPIC-1 - math (L1)

Slice-critical tickets first (needed for Milestone 1), then the wide independent set. ✅ = merged to `main`.

| ID | Component | Port | Acceptance | Size |
|---|---|---|---|---|
| ✅ QL-1.1 | Array | `ql/math/array.hpp` | `array.cpp` | M |
| QL-1.2 | Matrix + core matrixutilities | `ql/math/matrix.hpp`, `ql/math/matrixutilities/` (basics) | `matrices.cpp` | L → split (Matrix ops / decompositions) |
| ✅ QL-1.3a | ErrorFunction (`erf`) | `ql/math/errorfunction.{hpp,cpp}` - prerequisite for the Normal CDF | reference values across all `erf` regions (exercised via `distributions.cpp`) | M |
| ✅ QL-1.3 | Distributions - Normal | `ql/math/distributions/normaldistribution.*` (pdf/cdf/inverse) | `distributions.cpp` (normal cases) | M |
| QL-1.4 | Interpolations - Linear | `ql/math/interpolations/linearinterpolation.*` + `interpolation` base | `interpolations.cpp` (linear) | M |
| QL-1.5 | Solvers1D | `ql/math/solvers1d/` (Brent, Bisection, Newton, …) | `solvers.cpp` | M → 1 ticket per solver |
| QL-1.6 | Distributions - rest | bivariate normal, poisson, chi-square, gamma, … | `distributions.cpp` (rest) | L → split |
| QL-1.7 | Interpolations - rest | cubic/spline, loglinear, flat, 2D | `interpolations.cpp` (rest) | L → split |
| QL-1.8 | Integrals | `ql/math/integrals/` (segment, Simpson, GaussKronrod, Gauss-*) | `integrals.cpp` | L → split |
| QL-1.9 | Optimization | `ql/math/optimization/` (Simplex, LevenbergMarquardt, conjugate gradient, constraints) | `optimizers.cpp` | L → 1 ticket per optimizer |
| QL-1.10 | Statistics | `ql/math/statistics/` (general, risk, incremental, histogram) | `riskstats.cpp` | L → split |
| QL-1.11 | RNG - generators | `ql/math/randomnumbers/` MT19937, knuth, ranlux, box-muller, ziggurat (ALGORITHMS only) | `lowdiscrepancysequences.cpp` (rng part) | M |
| QL-1.12 | RNG - Sobol + data tables | `sobolrsg.cpp`, `primitivepolynomials.cpp`, `latticerules.cpp` (~115k LOC = static DATA) | `lowdiscrepancysequences.cpp` (sobol) | L → mechanical data transcription, script-assisted |
| QL-1.13 | ODE + copulas | `ql/math/ode/`, `ql/math/copulas/` | per-file cases | S each |
| QL-1.14 | Matrixutilities - decompositions | SVD, QR, Cholesky, pseudo-sqrt, symmetric schur | `matrices.cpp` (decomp) | L → 1 ticket per decomposition |

> ⚠️ **QL-1.12 note:** `randomnumbers` reads as 118k LOC but ~115k is static direction-integer / primitive-polynomial
> tables. The algorithm is small; the bulk is mechanical and should be transcribed with a generator script, not by hand.

---

## 7. EPIC-3 - quotes (L3, tiny - pulls into Milestone 1)

| ID | Component | Port | Acceptance | Size |
|---|---|---|---|---|
| QL-3.1 | Quote base + SimpleQuote | `ql/quote.hpp`, `ql/quotes/simplequote.*` | `quotes.cpp` | S |
| QL-3.2 | InterestRate + Compounding | `ql/interestrate.*`, `ql/compounding.hpp` | `interestrates.cpp` | M |
| QL-3.3 | Derived quotes | `ql/quotes/` (composite, derived, forward, eigenvalues, …) | `quotes.cpp` (rest) | S each |

---

## 8. Epics L4–L11 (headline only - break down after L0–L2 land)

Each becomes its own detailed ticket table once its dependencies are in place. Natural sub-epic boundaries:

- **EPIC-4 termstructures** → `yield/` (curves, bootstrap), `volatility/` (largest, 18k), `credit/`, `inflation/`.
  Oracle: `termstructures.cpp`, `swaptionvolatilitymatrix.cpp`, etc.
- **EPIC-5 processes** → `stochasticprocess` base, then `processes/` (BSM, Heston, HW, GSR, …).
- **EPIC-6 indexes** → `iborindex`, `swapindex`, ibor/swap families, inflation indices.
- **EPIC-7 cashflows** → coupon base, fixed/floating coupons, leg builders, cashflow vectors.
- **EPIC-8 instruments** → `instrument` base, `payoff`, `exercise`, `option`, vanilla options, `bonds/`, swaps.
- **EPIC-9 methods** → `montecarlo/`, `lattices/`, `finitedifferences/` (17k - large sub-epic).
- **EPIC-10 models** → `shortrate/`, `equity/` (Heston calibration), `marketmodels/` (24k - large sub-epic).
- **EPIC-11 engines** → `vanilla/` (14k), `barrier/`, `asian/`, `swaption/`, `basket/`, `bond/`, … (1 sub-epic per pricing family).

---

## 9. Execution notes

1. **Decide D1–D5 before writing porting code.** They are QL-0.1–0.6 and gate everything.
2. **Milestone 1 vertical slice before going wide** - derisk the architecture against `europeanoption.cpp`.
3. After the slice, **L0 → L1/L2 in parallel** (independent), then proceed down the layer map.
4. Every ticket ports its matching `test-suite/*.cpp` cases as Rust tests - the C++ outputs are the oracle.
5. Keep PRs ≤300 LOC; split any L-sized ticket as noted.
