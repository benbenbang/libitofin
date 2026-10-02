# Seeded Merton path oracle

These fixtures were generated independently on 2026-10-02 by the accompanying
Python standard-library script. It never imports libitofin, Rust/Python bindings,
NumPy or SciPy, and never obtains expected values from implementation outputs.
Its MT19937 initialization/twist/tempering follows the standard 32-bit algorithm
of Matsumoto and Nishimura; the first ten words for seed 5489 match the canonical
reference vector. Normal draws use Acklam's rational approximation, without
refinement. Counts use upward Poisson mass/CDF recurrence, not a normal approximation.

Each path starts at `spot`; with `dt=horizon/steps`, the exact transition is:

```text
kappa = expm1(mu + delta^2/2)
S_next = S * exp((drift - lambda*kappa - sigma^2/2)*dt
                + sigma*sqrt(dt)*Zd + N*mu + sqrt(N)*delta*Zj)
N ~ Poisson(lambda*dt)
```

The event intensity is the original lambda. Drift is expected arithmetic spot
growth including jumps; callers choose a forecast drift or risk-neutral r-q.
Diffusion seed is the request seed. Poisson/jump seeds shift by `0x9E3779B9`
and `0xBB67AE85` on the nonzero u32 ring:
`1 + ((seed - 1 + offset) % 4294967295)`. Uniforms are `(word+0.5)/2^32`.
Each step consumes one diffusion word, count word and aggregate-jump word,
including deterministic/zero-count/zero-horizon cases. Paths then steps advance
in row order. The full layout includes the initial spot and is flat `[path,time]`;
terminal values reproduce the last full column exactly, including float64 bits.
Zero intensity uses the same diffusion sequence as GBM; when diffusion also
vanishes, deterministic times use the exact horizon for the last step.

The two fixture JSON files cover twelve cases: actual multiple jumps, both log
mean signs, zero jump dispersion/intensity/diffusion/horizon, Poisson means
60 and 700 per step, another seed and maximum u32 seed. The base draw record
contains raw words, uniforms, normals, counts, CDF brackets and resulting spots.
All count uniforms are at least 0.000257 from their selected CDF boundaries.
The Poisson seed mass must stay normal (`exp(-mean)>=float64.MIN_POSITIVE`);
the reference rejects subnormal seeds and stalled CDFs instead of approximating.

The numerical fixture bound was fixed before any implementation comparison:
`abs(actual-expected) <= 2e-14*max(1,abs(expected))`, allowing roughly 90-180
float64 ULPs for Acklam/transcendental and at most four transition roundings.
Full versus terminal comparisons within an implementation must be bit exact.

For T=horizon, independent checks use `E[S]=S0*exp(drift*T)` and
`E[S^2]=S0^2*exp((2*drift+sigma^2+lambda*(exp(2*mu+2*delta^2)
-2*exp(mu+delta^2/2)+1))*T)`. Forty thousand paths pass six-standard-error
mean/second-moment tests (observed deviations 1.66/1.51); Poisson total counts
have observed mean 1.99245 and variance 2.004993 versus analytical 2.
Reproduce with `python sdk/go/testdata/generate_merton_paths_oracle.py`;
`--output DIR` writes elsewhere. No environment, compiler or dependency is needed.
