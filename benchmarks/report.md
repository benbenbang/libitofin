# itofin vs QuantLib: Benchmark Report

**Date:** 2026-10-26
**Packages:** itofin 0.45.0 (Rust/PyO3) vs QuantLib 1.44 (C++/SWIG)

## 1. Executive summary

| Question | Answer |
|---|---|
| **Same numbers?** | Yes, every workload matches to between 1e-12 and 1e-16 |
| **Safer?** | Yes, in the ways it claims: explicit errors, fixed known QuantLib bugs, no leaks found, no crashes under threads |
| **Faster?** | **Mixed.** Faster for option pricing and Heston; tie on swaptions; **slower on yield curves** |

**One-line verdict:** "Backed by Rust" doesn't mean faster everywhere. itofin wins on option-centric work; QuantLib's mature caching wins on curve workflows.

## 2. Environment and methodology

### Machines and versions

| Setup | Python | Arch | CPUs | Script |
|---|---|---|---|---|
| **1** | 3.13.14 | arm64 | n/a | Basic benchmark, 3 runs |
| **2** | 3.14.8 | arm64 | 28 | Stress test, scale 1 and 3 |
| **3** | 3.14.8 | arm64 | 28 | Stress + extra workloads, scale 1 |
| **4** | 3.14.8 | arm64 | 28 | Stress + extra workloads, scale 10 |

### Workloads

| # | Workload | Details |
|---|---|---|
| 1 | European, rebuild all | Build process + option + engine per strike; NPV + delta |
| 2 | Heston calibration | Setup 1: 9 helpers, order 96. Setups 2 to 4: 35 helpers, order 192 |
| 3 | Multi-thread | 28 threads, isolated process |
| 4 | Multi-process | 28 processes, spawn context |
| 5 | Memory soak | 50 repeated sweeps in a fresh process |
| 6a | European, market reused | Process/engine built once, only option per strike |
| 6b | Reprice at new spot | QuantLib bumps a `SimpleQuote`; itofin attaches a new process |
| 7 | Curve bootstrap | 3 Euribor deposits + 9 swaps, log-linear discount; plus bump 10Y quote and re-bootstrap |
| 8 | Swaption sweep | 1Y into 5Y payer, Hull-White + Jamshidian |

Timing: median of 5 to 7 runs after warm-up; CV% reported for stability.

## 3. Correctness results

| Check | Max abs diff / result | Verdict |
|---|---|---|
| European NPV (1,000 to 2,000 strikes) | 1.07e-14 | ✅ |
| European delta | 6.66e-16 | ✅ |
| European K=65 NPV | 2.1333684449 both | ✅ |
| Heston params (9 helpers) | sigma 0.000216, v0 0.010000, theta 0.010000 both | ✅ |
| Heston params (35 helpers) | sigma 0.000120, v0 0.010000, theta 0.010000 both | ✅ |
| Spot repricing checksum (1,000 spots) | 9.09e-13 | ✅ |
| Curve discount factors (59 times) | 3.33e-16 | ✅ |
| Swaption NPV (200 strikes) | 2.61e-13 | ✅ |
| Swaption K=3% NPV | 1.5666103956 both | ✅ |

## 4. Performance results

### Setup 1 (basic, 3 runs)

| Metric | itofin | QuantLib | itofin advantage |
|---|---|---|---|
| European, per option | 2.06 to 2.09 us | 18.07 to 18.42 us | 8.63 to 8.93x |
| Heston (9 helpers, order 96) | 7.50 to 7.52 ms | 13.24 to 13.52 ms | 1.76 to 1.80x |

### Setups 2 to 4: core tests

| Test | Scale 1 (S2) | Scale 3 (S2) | Scale 1 (S3) | Scale 10 (S4) |
|---|---|---|---|---|
| **European, itofin** | 2.26 us | 2.38 us | 2.41 us | 2.43 us |
| **European, QuantLib** | 21.28 us | 20.76 us | 20.90 us | 20.61 us |
| **Heston, itofin** | 78.2 ms | 79.8 ms | 82.1 ms | 80.7 ms |
| **Heston, QuantLib** | 137.5 ms | 138.9 ms | 135.0 ms | 135.7 ms |
| **Threads speed-up, itofin** | 0.93x | 0.98x | 1.00x | 1.00x |
| **Threads speed-up, QuantLib** | 1.02x | 0.98x | 1.00x | 0.98x |
| **Processes speed-up, itofin** | 9.61x | 14.66x | 11.60x | 18.21x |
| **Processes speed-up, QuantLib** | 19.39x | 19.95x | 18.13x | 20.47x |
| **Processes total, itofin** | 4.25M/s | 6.16M/s | 4.81M/s | 7.49M/s |
| **Processes total, QuantLib** | 0.91M/s | 0.96M/s | 0.87M/s | 0.99M/s |
| **Memory growth, both** | +0.1 MB | +0.0 MB | +0.1 MB | +0.0 MB |

### Setups 3 and 4: extra workloads

| Test | itofin (S3) | QuantLib (S3) | itofin (S4) | QuantLib (S4) |
|---|---|---|---|---|
| **European, market reused** | 1.07 us | 3.52 us | 1.16 us | 3.71 us |
| **Reprice at new spot** | 2.06 us | 1.75 us | 2.03 us | 1.85 us |
| **Curve build + bootstrap** | 1,253 us | 727 us | 1,219 us | 753 us |
| **Curve bump + re-bootstrap** | 721 us | 252 us | 708 us | 244 us |
| **Swaption (Jamshidian)** | 16.78 us | 18.24 us | 16.59 us | 18.19 us |

### Summary ratios (above 1.00x = itofin faster)

| Workload | Scale 1 | Scale 10 | Winner |
|---|---|---|---|
| European, rebuild all | 8.67x | 8.48x | itofin |
| Multi-thread European | 8.71x | 8.64x | itofin |
| Multi-process European | 5.55x | 7.54x | itofin |
| European, market reused | 3.29x | 3.20x | itofin |
| Heston calibration | 1.64x | 1.68x | itofin |
| Swaption sweep | 1.09x | 1.10x | Tie |
| Reprice at new spot | 0.85x | 0.91x | QuantLib (slightly) |
| Curve bootstrap | 0.58x | 0.62x | QuantLib |
| Curve bump + re-bootstrap | 0.35x | 0.34x | **QuantLib (~2.9x)** |

## 5. Key findings

### Finding 1: most of the ~9x option gap is API style

- Rebuilding QuantLib's ~8 objects per option is what makes it look 9x slower
- With market objects reused, the gap shrinks to **~3.2x**
- With QuantLib's quote-bump idiom, **QuantLib is slightly faster**

### Finding 2: QuantLib is clearly faster on curves

- Fresh bootstrap: QuantLib ~1.6x faster
- Bump + re-bootstrap: QuantLib ~2.9x faster
- QuantLib's re-bootstrap costs **~1/3** of a fresh build; itofin's costs **~3/5**
- **Hypothesis:** QuantLib seeds the solver with the previous solution; itofin may restart from scratch

### Finding 3: Heston numerics are genuinely faster in itofin

- ~1.65x across both small and large grids, where binding overhead is negligible

### Finding 4: threads don't help either library

- Both ~1.0x with 28 threads: both hold the GIL
- Neither crashed

### Finding 5: multi-process scaling gap was dispatch overhead

- itofin's speed-up rose from 9.6x (scale 1) to 18.2x (scale 10), close to QuantLib's 20.5x
- Short tasks penalized the faster library; not a real scaling limit

### Finding 6: no memory leaks

- Flat RSS in all soak runs
- Not tested: itofin's known callback-cycle issue (#1028)

## 6. Recommendations

| Workload | Recommended |
|---|---|
| Pricing many new options from Python | **itofin** (3 to 9x) |
| Heston / model calibration | **itofin** (~1.65x) |
| Swaptions | Either |
| Live repricing as quotes tick | **QuantLib** (slightly) |
| Yield curves and curve risk (DV01, bumps) | **QuantLib** (1.6 to 2.9x) |
| Broad coverage, production stability | **QuantLib** (itofin is pre-1.0) |
| Parallelism | Processes with either; not threads |

### Caveats

- Single machine type (Apple Silicon, arm64); x86 may differ
- Only tested itofin features that exist today
- Optimizer iteration counts weren't compared for Heston

## 7. GitHub issue draft

Repo: **https://github.com/benbenbang/libitofin/issues** (search "bootstrap" first in case it's already reported)

````markdown
**Title:** Perf: PiecewiseYieldCurve re-bootstrap after a quote bump is ~2.9x slower than QuantLib

## Summary

Bootstrapping a `PiecewiseYieldCurve` from deposits and swaps produces results
identical to QuantLib (max discount-factor diff 3.33e-16), but it's slower:

- Fresh build + bootstrap: ~1.6x slower than QuantLib
- Bump one quote + re-bootstrap: ~2.9x slower than QuantLib

The relative cost of a re-bootstrap is the most notable part. In QuantLib it's
about 1/3 of a fresh build; in itofin it's about 3/5. This suggests the
iterative bootstrap may not reuse the previous solution as the starting guess
after a quote change (QuantLib's appears to). Happy to be corrected if the
cause is elsewhere.

This matters for curve risk: bucketed DV01 and scenario runs do many
bump + re-bootstrap cycles, so the gap compounds.

## Environment

- itofin 0.45.0 (PyPI wheel), QuantLib 1.44 (PyPI wheel)
- Python 3.14.8, macOS arm64 (28 cores)
- Results stable across runs (CV 0.7% to 4.3%)

## Results (median of 5 runs, microseconds)

| Test | itofin | QuantLib | Ratio |
|---|---|---|---|
| Build + bootstrap | 1,219 to 1,253 | 727 to 753 | QL ~1.6x faster |
| Bump 10Y quote + re-bootstrap | 708 to 721 | 244 to 252 | QL ~2.9x faster |

For context, itofin is faster elsewhere in the same benchmark: ~8.5x on
European option build + price, ~1.65x on Heston calibration, and ties on
Jamshidian swaptions. So this looks specific to the bootstrap path.

## Curve setup

- Reference date 2026-01-15, Actual/365 Fixed, log-linear discount (default)
- Deposits: Euribor 1M / 3M / 6M at 3.00% / 3.10% / 3.20%
- Swaps vs 6M Euribor, annual fixed, Unadjusted, 30/360 Bond Basis, TARGET:
  1Y 3.30%, 2Y 3.40%, 3Y 3.45%, 5Y 3.50%, 7Y 3.55%, 10Y 3.60%,
  15Y 3.65%, 20Y 3.68%, 30Y 3.70%
- Bump: the 10Y swap quote alternates +1bp / +2bp, then `discount(10.0)` is
  queried to force the re-bootstrap

## Minimal repro

```python
import statistics
import time

import QuantLib as ql
from itofin import Settings
from itofin.indexes import Euribor
from itofin.quotes import SimpleQuote
from itofin.termstructures import DepositRateHelper, PiecewiseYieldCurve, SwapRateHelper
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Period

DEPOSITS = ((1, 0.0300), (3, 0.0310), (6, 0.0320))
SWAPS = ((1, 0.0330), (2, 0.0340), (3, 0.0345), (5, 0.0350), (7, 0.0355),
         (10, 0.0360), (15, 0.0365), (20, 0.0368), (30, 0.0370))
BUMP = len(DEPOSITS) + 5  # 10Y swap


def it_curve():
    s = Settings()
    today = Date(15, 1, 2026)
    s.set_evaluation_date(today)
    idx = Euribor(Period(6, "Months"), None, s)
    quotes, helpers = [], []
    for m, r in DEPOSITS:
        q = SimpleQuote(r)
        quotes.append(q)
        helpers.append(DepositRateHelper(q, Euribor(Period(m, "Months"), None, s)))
    for y, r in SWAPS:
        q = SimpleQuote(r)
        quotes.append(q)
        helpers.append(SwapRateHelper(q, Period(y, "Years"), Calendar.target(),
                                      Frequency.Annual, BusinessDayConvention.Unadjusted,
                                      DayCounter.thirty360_bond_basis(), idx))
    return s, PiecewiseYieldCurve(today, helpers, DayCounter.actual365_fixed()), quotes


def ql_curve():
    today = ql.Date(15, ql.January, 2026)
    ql.Settings.instance().evaluationDate = today
    idx = ql.Euribor6M()
    quotes, helpers = [], []
    for m, r in DEPOSITS:
        q = ql.SimpleQuote(r)
        quotes.append(q)
        helpers.append(ql.DepositRateHelper(ql.QuoteHandle(q), ql.Euribor(ql.Period(m, ql.Months))))
    for y, r in SWAPS:
        q = ql.SimpleQuote(r)
        quotes.append(q)
        helpers.append(ql.SwapRateHelper(ql.QuoteHandle(q), ql.Period(y, ql.Years), ql.TARGET(),
                                         ql.Annual, ql.Unadjusted,
                                         ql.Thirty360(ql.Thirty360.BondBasis), idx))
    return helpers, ql.PiecewiseLogLinearDiscount(today, helpers, ql.Actual365Fixed()), quotes


def bump_loop(build, set_value, n=500):
    keep, curve, quotes = build()
    curve.discount(1.0)  # initial bootstrap
    q = quotes[BUMP]
    base = q.value()
    t0 = time.perf_counter()
    for i in range(n):
        set_value(q, base + 0.0001 * (i % 2 + 1))
        curve.discount(10.0)  # forces re-bootstrap
    return (time.perf_counter() - t0) / n


def median_us(fn, repeat=5):
    fn()  # warm-up
    return statistics.median(fn() for _ in range(repeat)) * 1e6


it = median_us(lambda: bump_loop(it_curve, lambda q, v: q.set_value(v)))
qq = median_us(lambda: bump_loop(ql_curve, lambda q, v: q.setValue(v)))
print(f"itofin   {it:8.1f} us per bump + re-bootstrap")
print(f"QuantLib {qq:8.1f} us per bump + re-bootstrap")
print(f"QuantLib is {it / qq:.2f}x faster")
```

## Expected

Re-bootstrap after a small quote change should be noticeably cheaper than a
fresh build (closer to QuantLib's ~1/3 ratio), ideally by warm-starting each
node's solve from the previous solution.

## Possible directions

- Seed the per-node 1-D solver with the previously bootstrapped value when
  the curve is invalidated by a quote change
- Profile whether helper repricing (schedule/leg construction per solver
  iteration) dominates the cost
````
