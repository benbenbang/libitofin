# GJR-GARCH model, calibration and pricing oracle

Issue [#1165](https://github.com/benbenbang/libitofin/issues/1165) extends the
[process oracle](gjrgarch-oracle.md) with independent models and engines.

## Provenance and regeneration

- Numerical outputs use native **QuantLib-Python 1.43**, never itofin outputs.
- Contract source is QuantLib commit `9863b578af0caa4cecabf697196533e84a8308b6`.
  The published binary is version-pinned, not claimed to be built from that commit.
- Run `python sdk/go/testdata/generate_gjrgarch_model_oracle.py` with QuantLib 1.43.
  `--output DIRECTORY` supports a clean second regeneration for byte comparison.
- Fixtures use reference/evaluation date **2026-01-15** and Actual365Fixed,
  except the original matrix uses ActualActual ISDA and DAX uses **2002-07-05**.
  Flat curves are fixed-reference, continuously compounded. Expiries add calendar
  days except DAX TARGET helpers, which advance rounded week periods.
- The local oracle virtual environment only reads an existing cache through a
  `.pth` entry. No cache package, project implementation or user environment changes.

| Pinned source | SHA256 |
| --- | --- |
| `ql/models/equity/gjrgarchmodel.cpp` | `14d8c3f86614d16b28a39eafe00812805c6b6006d49c281adb9976bf23ef5b5a` |
| `ql/models/equity/gjrgarchmodel.hpp` | `0287fa9fa40afa9644d4c28a49138731f2dd61be11b8efeaa7614c67c64d605b` |
| `ql/pricingengines/vanilla/analyticgjrgarchengine.cpp` | `4851498ea4f64363f6a61fe15911c865950023e3b3d62b13883db1d7b15e14d0` |
| `ql/pricingengines/vanilla/mceuropeangjrgarchengine.hpp` | `cb1a75a261142070b4821f9a68df262a3f7374d940508d59191818676b3db99d` |
| `test-suite/gjrgarchmodel.cpp` | `1021dd54dfa3d65c2b43cb9eae59df64cbf332e61c4a9b519351f5f980023f7d` |

Local cached native `_QuantLib.abi3.so` SHA256:
`04a53b6f425e728083149241f77e9b9711db845614ce65d2544a09a0970597d8`.
Package `quantlib-1.43.dist-info/METADATA` SHA256:
`5552ea247ebb763eac0e05b675d6c2a92d270fd0da4b7eea8d8826fc0b308fea`.
These identify the independent executable used, not a cross-platform binary hash.

## Fixture map

| File suffix | Evidence |
| --- | --- |
| `matrix-0`, `matrix-1`, `matrix-2` | Original 36 calls, independent puts, original rounded analytic/MC caches, native seeded MC prices/errors and replayed sample counts |
| `analytic` | 12 call/put cases, signed lambda, nonzero dividends, negative gamma/rates, nonstationarity, nonintegral rounded days, 1-365 day horizons and live bumps |
| `mc` | 12 seeded call/put rows across three schemes and antithetic modes; independent tolerance stopping count |
| `dax` | Original 21 TARGET/ZeroCurve helpers, market prices, calibrated references and percentage-volatility SSE |
| `synthetic`, `synthetic-free-omega` | Six independent synthetic implied-volatility helpers, unequal weights and five/four fixed parameters |
| `edge` | Native singular results, model discretization reset and native setter/constraint distinction |

`input` uses daily `daily_variance` and omega; all parameter arrays are ordered
`[omega, alpha, beta, gamma, lambda, daily_variance]`. `changes` overrides a shared
input. Option types are Call `1`, Put `-1`. MC `settings` names follow native SWIG;
antithetic sample counts count **pairs**, not twice the number of accumulations.

## Numerical contracts and limits

- The analytic engine is an Edgeworth approximation, not exact MC pricing.
  The original test compares analytic results to `analytic` caches and MC results
  to distinct `mcValues` caches with `2 * 0.075 = 0.15` absolute bands. It does
  **not** compare analytic and MC prices to one another. The generator retains
  both gates without widening them. Native high-precision values permit tighter
  port comparisons, typically absolute `1e-10` for these price rows.
- The dividend convention is unusual: analytic calls are not multiplied by the
  dividend discount. Native parity is `call - put = spot - strike * Dr / Dq`.
  This must not be replaced by standard discounted spot parity silently.
- Native analytic caching omits the carry rate from its cache key. A live rate
  quote update can reuse stale call moments. `cumulative_live_bumps.price` uses a
  **fresh native engine** after each cumulative bump; `native_cached_price`
  records the old engine's observed defect. Correct live pricing should compare
  against the fresh values, not reproduce this cache bug.
- Zero strike and singular coefficient moments produce native NaN. `edge` encodes
  this as the string `nan`, not invalid JSON. A zero initial daily variance is
  rejected by native model construction. These are domain evidence, not valid
  finite prices or justification to return fabricated values.
- Model construction regenerates its process and resets Reflection to default
  FullTruncation. A negative-variance zero-time transition independently exposes
  the reset. Market handles and days/year are retained.
- Calibration constraints: omega/v0 strictly positive; alpha/beta in [0,1]; gamma
  in [-1,1]; lambda unconstrained; beta+gamma nonnegative. No stationarity bound.
  Native `setParams` bypasses these checks. Atomic validating setters are a safer
  deliberate policy, not claimed native setter equivalence.
- Native sample count is not exposed by SWIG. Recorded tolerance counts are
  independently recovered by replaying fixed-sample native engines with identical
  seed/steps and the pinned adaptive-batch rule until price **and error** match
  exactly. The standalone tolerance case stops at **12,095** observations, price
  `2.9423120714228577`, error `0.03921333473521664` under tolerance `0.04`.

## Calibration reproduction

### Original DAX test

- Evaluation/settlement: 2002-07-05, TARGET, Actual365Fixed; spot 4468.17, dividend 0.
- Linear ZeroCurve interpolation in continuously compounded zero rates; all nine
  dates/rates are recorded. Use this curve, not a single flat-rate substitute.
- Original volatility surface strike indices 3-9 and maturity indices 0-2:
  strikes 4000-5000, source tenors 13/41/75 days rounded to 2/6/11 weeks.
  TARGET expiries are 2002-07-19, 2002-08-16 and 2002-09-20: **14/42/77 days**.
- Helpers select OTM call/put using forward moneyness and `ImpliedVolError`.
- Native optimizer: Simplex lambda `0.05`; EndCriteria
  `(400, 40, 1e-8, 1e-8, 1e-8)`; equal weights, all six parameters free.
- Independent percentage-volatility SSE: **7.852200556326041**, below original 15.
  SSE is `sum((100 * calibrationError)^2)`, not price SSE. Native evaluations: 604.
- Native calibrated parameters and every helper residual are recorded. Solver
  trajectories/platform rounding may differ, so success is the unchanged SSE
  bound and repricing, not exact equality of a nonunique parameter vector.

### Deterministic synthetic fit

- Native analytic prices generate six OTM contracts at 14/35/63 days and 90/110
  strikes with dividend zero. Native implied-volatility inversion uses accuracy
  `1e-12`, maximum 1000 evaluations, bracket `[0.001, 5]`.
- Start v0 at 70% of target; fix omega/alpha/beta/gamma/lambda. Weights `[1,2,3,4,5,6]`.
- Simplex lambda `0.00002`, EndCriteria `(500,40,1e-10,1e-10,1e-10)`.
  Native v0 fits to `0.00015873012966579863`; the five fixed parameters remain
  bit-identical. The fixture records weighted implied-vol SSE and helper reprices.
- A second fit also frees omega, starting both omega/v0 at 70% of target. Native
  omega `0.0000019999954810951823`, v0 `0.00015873026667458383`, weighted SSE
  `8.963190781594514e-15`; four fixed parameters remain bit-identical.

## Attribution

The original engine/cache tables and DAX input selection are from QuantLib's
`test-suite/gjrgarchmodel.cpp`, Copyright (C) 2008 Yee Man Chan, distributed under
[QuantLib's license](https://www.quantlib.org/license.shtml). The test attributes
its DAX example to A. Sepp, *Pricing European-Style Options under Jump Diffusion
Processes with Stochastic Volatility: Applications of Fourier Transform*.
Independent high-precision native outputs are regenerated by the three adjacent
`generate_gjrgarch_model_*oracle*.py` scripts. No QuantLib implementation is copied
into these generators.
