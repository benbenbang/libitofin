# Status and scope

[Project index](../README.md)

## Releases and binding delivery

Rust, Python, and Go are published together. The Go module uses a matching
native C ABI package; Node remains a placeholder. See the
[release history](https://github.com/benbenbang/libitofin/releases),
[Go installation](go.md), and [Python guide](python.md).

Go delivery [#1000](https://github.com/benbenbang/libitofin/issues/1000),
parity [#1030](https://github.com/benbenbang/libitofin/issues/1030), and behavioral
follow-ups [#1036](https://github.com/benbenbang/libitofin/issues/1036) are closed.
API accounting, numerical tests, and statement coverage measure different things;
see the [Go validation record](../docs/go-binding-test-gaps.md).

## Credit extensions

- [#1023](https://github.com/benbenbang/libitofin/issues/1023): spread and upfront
  helpers support ISDA pricing, extended pillars and quote recalibration in Rust,
  Python and Go. [QuantLib fixtures](../crates/libitofin/tests/fixtures/isda_helpers/generator.py)
  independently pin prices and settings-sensitive settlement cases.
- [#1021](https://github.com/benbenbang/libitofin/issues/1021): fixed and moving
  FlatHazardRate curves accept observable survival jumps in Rust, Python and Go.
  Other Rust curves can opt into the shared jump hook. Strict boundaries,
  generated year-end dates and date updates match the
  [QuantLib oracle](../sdk/go/testdata/credit_jumps_oracle.md).
- [#1020](https://github.com/benbenbang/libitofin/issues/1020): default-density
  interpolation and CDS bootstrapping support BackwardFlat and Linear in Rust,
  Python and Go. [Analytic and binding tests](../crates/itofin-py/tests/test_credit_density.py)
  cover survival, density, quote recalibration and retained dependencies.
- [#1024](https://github.com/benbenbang/libitofin/issues/1024): Rust, Python and Go
  expose the CDS protection end as the final coupon's accrual end, independent of
  payment-date adjustment. [Cross-language regression](../crates/itofin-py/tests/test_credit_protection_end.py).
- [#1022](https://github.com/benbenbang/libitofin/issues/1022): custom Rust hazard
  curves can derive survival using QuantLib's 48-point Gauss-Chebyshev fallback.
  Existing concrete curves retain their closed-form calculations. The
  [independent oracle generator](../crates/libitofin/tests/fixtures/credit_hazard_quadrature.py)
  pins the quadrature result separately from its analytical integration error.

## Inflation extensions

[#1018](https://github.com/benbenbang/libitofin/issues/1018) adds monthly Kerkhof
seasonality in Rust, Python and Go. The cumulative correction matches 72
[independent QuantLib rows](../crates/libitofin/tests/fixtures/kerkhof_seasonality.md);
installing or clearing it recalibrates zero-inflation curves. Exactly twelve
factors are supported; YoY corrections are rejected. This addition is on main
and will ship after v0.25.0.

[#1017](https://github.com/benbenbang/libitofin/issues/1017) adds lazy zero-inflation
base dates. Python/C/Go expose a last-fixing constructor with linear interpolation;
its unlinked index copy shares settings/history without retaining its forecast
curve. Generic callbacks remain Rust-only. `try_base_date()` and binding inspectors
propagate calculation errors; legacy Rust `base_date()` returns null on failure.
The [oracle](../crates/libitofin/tests/fixtures/lazy_inflation_base/README.md) compares
rebuilt node grids with fresh QuantLib curves and documents its persistent-grid
difference. Existing fixed-base constructors remain available.

## Joint curve bootstrapping

Rust exposes `IborIborBasisSwapRateHelper` and the genuinely coupled 3M/6M
`MultiCurve` oracle ([#995](https://github.com/benbenbang/libitofin/issues/995)).
Both helper sets read the opposite curve; QuantLib's FRA and swap repricing
tolerances are preserved. Fixing-history updates on either index invalidate
and recalibrate the live curve without observing its own forecast handle.
This work will ship after v0.25.0. Concrete Python/C/Go joint-curve assembly
remains [#1066](https://github.com/benbenbang/libitofin/issues/1066); the overnight
basis-helper sibling remains [#1060](https://github.com/benbenbang/libitofin/issues/1060).

## Hull-White calibration

[#400](https://github.com/benbenbang/libitofin/issues/400) covers fixed-reversion
and zero-start-delay calibration. Rust tests both coupon modes; Python and Go
check the exposed PAR cases against the original QuantLib caches at `1e-5`,
including calibration after input wrappers are released.
[Binding regressions](../crates/itofin-py/tests/test_hullwhite_calibration.py).

## Swaption foundation

[EPIC-10 #358](https://github.com/benbenbang/libitofin/issues/358) covers the
constant volatility surface, swaption instrument, Black engine, Eonia, SwapIndex
and vanilla MakeSwaption builder. Its five core issues (#359-#363) are complete.
Physical and cash settlement, both cash annuity models, and payer/receiver pricing
are supported; the later convention and delta gates (#364/#365) are also complete.

| Evidence | Independent price checks |
| --- | --- |
| [Rust Black engine](../crates/libitofin/src/pricingengines/swaption/blackswaptionengine.rs) | Par `0.036418158579`, indexed `0.036421429684`, Eonia OIS `0.014101075767`; absolute tolerance `1e-12` |
| [Python oracle](../crates/itofin-py/tests/test_black_swaption_oracle.py) | Cached par value, 12 QuantLib 1.43 settlement cases, retained dependencies and live quote repricing |
| [Go settlement oracle](../sdk/go/rates_completion_settlement_test.go) | Same 12 independent settlement cases; both cash annuity models and payer/receiver sides |

Python/C/Go expose Eonia, OIS-underlying swaptions and the vanilla MakeSwaption
convenience ([#1049](https://github.com/benbenbang/libitofin/issues/1049)).
[Python](../crates/itofin-py/tests/test_swaption_facades.py) and
[C/Go](../sdk/go/swaption_facades_test.go) tests preserve the Eonia OIS cached NPV
`0.014101075767` at `1e-12`, independent forecast pins, exercise-calendar overrides,
retained dependencies and live repricing. These facades will ship after v0.25.0.
MakeSwaption is payer-only and has no overnight-index builder; SwapIndex clone
variants and swaption implied volatility remain deferred.

[#570](https://github.com/benbenbang/libitofin/issues/570) completes all five
swaption volatility matrix constructors in Rust, Python and Go, including fixed
live quotes, moving numeric values and explicit option dates. The
[independent oracle](../crates/libitofin/tests/fixtures/swaption_matrix/README.md)
pins 120 volatility nodes at `1e-16`; Rust also checks Black prices, volatility
recovery through a test-only flat engine, and quote-handle relinks.

Rust supports backward-flat SABR parameter cubes
([#606](https://github.com/benbenbang/libitofin/issues/606)), with independent
QuantLib sparse/dense oracles and live quote/date recalculation checks. Both
axes require at least two nodes. This feature will ship after v0.25.0;
Python/C/Go flag exposure remains [#1065](https://github.com/benbenbang/libitofin/issues/1065).
SABR variants [#586](https://github.com/benbenbang/libitofin/issues/586) and ZABR
[#597](https://github.com/benbenbang/libitofin/issues/597) retain separate scope.

## Dependency layers

The port proceeds **bottom-up** through dependency layers L0→L11; each layer
depends only on lower-numbered layers. The live backlog is the
[GitHub Project board](https://github.com/users/benbenbang/projects/5) and
[open issues](https://github.com/benbenbang/libitofin/issues). Use those
for current acceptance criteria; the layer map below describes scope, not full
QuantLib parity.

| Layer | Epic | Scope |
|------|------|-------|
| **L0** | core | types, errors, patterns, settings, handle, utilities |
| **L1** | math | array/matrix, distributions, interpolation, integrals, solvers, optimization, statistics, RNG, ODE, copulas, decompositions |
| **L2** | time | `Date`, `Period`, `Calendar`, `DayCounter`, `Schedule`, IMM/ASX/ECB |
| **L3** | quotes | `Quote`, `SimpleQuote`, derived quotes, `InterestRate`, compounding |
| **L4** | term structures | interpolated yield curves, Black-vol curves/surfaces, local vol, swaption vol surfaces (matrix / interpolated / SABR cube), cap-floor term-vol surfaces + optionlet stripping |
| **L5** | processes | multi-factor `StochasticProcess` / `StochasticProcess1D`, Black-Scholes, Heston (analytic surface), Ornstein-Uhlenbeck, correlated process array |
| **L6** | indexes | `InterestRateIndex`, Ibor family (Euribor / Eonia / €STR / SOFR), `SwapIndex` |
| **L7** | cashflows | fixed / floating / Ibor / overnight coupons and legs, coupon pricers, duration, capped-floored coupons |
| **L8** | instruments | fixed-rate bonds, vanilla / OIS swaps, swaptions, caps & floors, vanilla options |
| **L9** | methods | lattices, trees (trinomial + Hull-White), Monte Carlo (path generators + antithetic), finite differences (European, American and Bermudan Black-Scholes vanilla) |
| **L10** | models | `CalibratedModel` + `calibrate()`, short-rate (Vasicek, CIR, Hull-White), Heston, calibration helpers |
| **L11** | engines | analytic European & Heston (Fourier), swaption (Black / Bachelier / Jamshidian), discounting swap / bond, Black cap/floor |

**Milestone 1 (done):** a European option prices end-to-end - quote → flat
yield/vol curves → generalized Black-Scholes process → analytic engine → lazy
instrument greeks - matching QuantLib's `europeanoption.cpp` value and greeks to
double-rounding precision, with the full observer/invalidation graph exercised.

Verified end-to-end since then, each against the matching `test-suite/` oracle:

- **Hull-White calibration** - a Hull-White model calibrates to a swaption strip
  (Jamshidian decomposition + Levenberg-Marquardt), reproducing
  `shortratemodels.cpp`'s cached mean-reversion and volatility.
- **Heston analytic + calibration** - the Fourier `AnalyticHestonEngine` prices
  European options to `hestonmodel.cpp`'s cached values, and a Heston model
  calibrates to a DAX volatility surface (reproducing the reference SSE).

### What's usable today

- **`types` / `errors`** - QuantLib's numeric aliases and `QlError` / `QlResult`
  with `fail!` / `require!` macros (the analogue of `QL_FAIL` / `QL_REQUIRE`).
- **`patterns` / `handle` / `settings`** - the observer/observable graph,
  `LazyObject`, `Handle` / `RelinkableHandle`, and the evaluation-date context.
- **`math`** - arrays and matrices (with SVD/QR/Cholesky/…), the distribution
  family, interpolation (linear → bicubic), integrals (incl. Gauss quadratures),
  1-D solvers, optimizers, statistics, RNGs (MT/Sobol/…), ODEs, copulas.
- **`time`** - dates, periods, 50+ calendars, day counters, schedules, IMM/ASX/ECB.
- **`quotes` / `interestrate`** - simple and derived quotes, interest-rate and
  compounding conversions.
- **`termstructures`** - flat and interpolated yield curves (zero/discount/
  forward), implied and spreaded curves, Black-variance curves/surfaces, local vol,
  swaption vol surfaces (matrix / interpolated / SABR cube) and cap-floor term-vol
  surfaces with optionlet stripping (SABR smile sections + calibrated interpolation).
- **`indexes`** - `InterestRateIndex`, the Ibor family (Euribor / Eonia / €STR /
  SOFR) and `SwapIndex`, with fixings threaded through `Settings` (D11).
- **`cashflows`** - fixed, floating, Ibor and overnight coupons and legs, coupon
  pricers, duration, and capped-floored coupons.
- **`processes`** - the multi-factor `StochasticProcess` base, generalized
  Black-Scholes, Heston (analytic surface), Ornstein-Uhlenbeck, and a correlated
  process array.
- **`models`** - `CalibratedModel` with `calibrate()`, the short-rate family
  (Vasicek, CIR, Hull-White), the Heston model, and calibration helpers
  (swaption, Heston).
- **`instruments` / `pricingengines`** - vanilla payoffs and exercise,
  `EuropeanOption`, fixed-rate bonds, vanilla / OIS swaps, swaptions, caps &
  floors; the analytic European and Heston (Fourier) engines, the swaption
  engines (Black / Bachelier / Jamshidian), and discounting swap / bond engines.
