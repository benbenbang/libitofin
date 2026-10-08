# Third-party notices

`itofin-optimize` is an independent implementation written from the published
papers listed in the crate documentation. It has one runtime dependency,
`thiserror`, and adapts no third-party source.

## Inspected for design comparison only

The two crates below were read while choosing this crate's public API shape.
Nothing was copied, adapted or translated from either, and no clean-room
procedure was followed, so no clean-room claim is made here.

| Crate | Version | License | Source |
| --- | --- | --- | --- |
| argmin | 0.11.0 | MIT OR Apache-2.0 | https://github.com/argmin-rs/argmin |
| optimization | 0.2.0 | MIT | https://github.com/b52/optimization-rust |

## Differential evolution

The DE/rand/1/bin solver is independently written from the mathematical algorithm
in Storn and Price (1997), DOI <https://doi.org/10.1023/A:1008202821328>. Its
SplitMix64 recurrence uses the published 64-bit arithmetic description, with our
own bounded-index sampling and population code. No SciPy, QuantLib, Argmin or
optimization-rust differential-evolution source or tests were adapted.

## Particle swarm

The global-best PSO solver is independently written from the mathematical
algorithm in Kennedy and Eberhart (1995), DOI
<https://doi.org/10.1109/ICNN.1995.488968>, and the inertia formulation in
Shi and Eberhart (1998), DOI <https://doi.org/10.1109/ICEC.1998.699146>.
The original 1995 paper is available from
<https://staff.washington.edu/paymana/swarm/kennedy95-ijcnn.pdf>.
Synchronous updates, zero initial velocities, clipping with absorbing bounds,
strict best-update ties and velocity-aware convergence are our explicit policies.
No QuantLib, Argmin or other upstream particle-swarm implementation or tests
were copied, adapted or translated. This is not a clean-room claim.

## SciPy

SciPy is used at development time only, by `scripts/fixtures/optimize/gen_fixtures.py`,
to generate reference values that are checked in as JSON with the SciPy version
recorded. SciPy is never a dependency of this crate, and its SLSQP
implementation (the ACM TOMS 733 Fortran and its C translation) is deliberately
not read or adapted in any language.
