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
per-generator, finite and positive; Python defaults to one. Seed zero selects a
random MT19937 seed. Copying preserves state and subsequent draws are independent.
Sequence results are copies; the last successful sample survives a failed draw.

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
