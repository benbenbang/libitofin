# Random generation policies

Version 0.27.0 adds European QMC pricing across Rust, Python, C and Go.
Rust uses `MakeMcEuropeanEngine::<LowDiscrepancy>`; Python and Go expose
`QMCEuropeanEngine`, and the C engine factory selects kind 3. All use
Sobol/Jaeckel points transformed to standard normal variates. Supply positive
fixed samples and either positive steps or steps per year. Statistical error
estimates, absolute tolerances and `max_samples` are unavailable for QMC.
Existing Monte Carlo constructors continue to use `PseudoRandom`.

The QuantLib `testQmcEngines` grid covers 108 European calls/puts with one
step and 4095 samples. Its acceptance is `abs(QMC - analytic) / spot <= 0.01`.
The first Sobol uniform point is 0.5 in every dimension, so the first Gaussian
point is zero. Sobol skips the all-zero point; copied generators preserve their
current positions. QMC rejects sample counts above the `2**32 - 1` Sobol period.

Python and Go also expose scalar and sequence Poisson generators. Rates are
per-generator, finite and positive; Python defaults to one. Seed zero follows
the table below. Copying preserves state and subsequent draws are independent.
Sequence results are copies; the last successful sample survives a failed draw.

## What seed 0 means

Seed `0` keeps QuantLib's meaning for pseudo-random generators: a random seed
drawn from the process-wide seed generator. Unlike QuantLib, the drawn seed is
reported, so a random run can be replayed bit for bit by passing it back. The
reported seed is never `0`, because `0` would draw again.

| Generator | Seed `0` | Seed reported |
| --- | --- | --- |
| MT19937, Knuth, Xoshiro256** | Random seed, redrawn until nonzero | Rust `seed()`; Python `.seed()`, C `itofin_rng_seed`, Go `Seed()` for MT |
| Box-Muller, inverse-cumulative, uniform/Gaussian sequences, Poisson | As the wrapped generator | Forwarded from the wrapped generator |
| MT built from a seed array | Not applicable | Rust/Python `None`; C and Go `0` |
| Ranlux | Fixed seed 19780503, as QuantLib | Not reported |
| Sobol | Used literally: it only seeds dimensions above the tabulated count | Not reported |
| Halton | No seed: the sequence is deterministic | Not applicable |
| Pseudo-random MC engines (European, Heston, GJR-GARCH, American LSM, variance swap) | Random seed, drawn once per calculation | Additional result `"seed"` (a real) |
| American LSM calibration | Derived from the resolved pricing seed, never `0` | Additional result `"seed_calibration"` |
| Low-discrepancy (QMC) engines | Kept as `0`: the run stays deterministic | Additional result `"seed"`, the given seed |
| Path kernels (`simulate_gbm`, GJR/OU/Merton/Heston paths) | Rejected | Not applicable: the seed is always explicit |

Read `"seed"` from an option's `results()` in Python (including after the
one-shot `price_mc`, which returns only the NPV), `Results().AdditionalResults`
in Go, or the C results snapshot. Variance swap MC records `"seed"` in the Rust
core only: its Python, C and Go bindings have no results snapshot yet, so the
seed is not reachable there.

LSM divergence from QuantLib (`mclongstaffschwartzengine.hpp`): QuantLib draws
two independent random seeds for pricing and calibration. Here the calibration
seed is derived from the resolved pricing seed, so the one pricing seed reproduces
both runs. The single pricing seed whose derived calibration seed wraps to `0`
uses calibration seed `1` instead.

Rust supplies `InverseCumulativeRng`, `FallibleInverseCumulativeRsg`,
`GenericLowDiscrepancy`, `LowDiscrepancy` and `PoissonPseudoRandom`.
The explicit inverse-transform constructors replace QuantLib's mutable global
`icInstance`. Poisson sequences return `QlResult` and intentionally do not
implement the infallible `McRngTraits` contract: an extreme quantile can be
numerically unresolved. Failed transformations consume the attempted uniform
draw and report the error. The normal fallible transform rejects endpoints;
Poisson accepts zero and rejects one. Existing infallible Gaussian APIs remain
unchanged. Rates whose exponential seed underflows are rejected.

Sources: `QuantLib/ql/math/randomnumbers/rngtraits.hpp`,
`QuantLib/test-suite/rngtraits.cpp`, and
`QuantLib/test-suite/europeanoption.cpp:testQmcEngines`.
