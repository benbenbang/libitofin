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
