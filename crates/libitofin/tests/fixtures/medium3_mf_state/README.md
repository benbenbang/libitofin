# MfStateProcess oracle provenance

- Original QuantLib source: commit `9863b578af0caa4cecabf697196533e84a8308b6`.
- Inspected `ql/processes/mfstateprocess.hpp` and `.cpp`, including empty-grid
  variance, raw diffusion, upper-bound buckets, signed reversion and epsilon guards.
- `native.tsv`: 51 observations from compiling that actual C++ class, not a copied
  formula. `native.cpp` calls the original process. Normal zero/positive/negative
  reversion, boundary and terminal buckets, zero duration and empty-grid cases.
- `reference.tsv`: 15 observations independently evaluated using Python Decimal
  with 80-digit precision and the exponential antiderivative in pinned source.
  This oracle does not import Rust or implement its anchored expm1/log algorithm.
  Inputs are converted from binary64 exactly; interval endpoints use binary64
  addition, matching the native time API. Values retain 55 significant digits.
- QuantLib PyPI 1.43 is a different revision and does not export `MfStateProcess`.
  It was inspected for availability, but is not used to generate either fixture.
- Near-zero and extreme scaling references deliberately replace cancellation,
  overflow or underflow artifacts in native double-precision arithmetic with
  accurate values for its source-defined variance law. This is numerical
  stabilization, not a claim of bitwise native parity at those extremes.
- Rust checks fixed `2e-13` relative tolerance, with a minimum-normal floor for
  subnormal results. Existing repository tolerances are unchanged.

Regenerate without external Python packages:

```sh
python3 generate.py
python3 generate.py --quantlib /path/to/pinned/QuantLib --boost-include /path/to/boost/include
```

The second command requires Clang and Boost headers. On this macOS host, source
`/private/tmp/itofin-medium-wave3-20261011/env.sh` to select the compatible SDK.
No QuantLib installation or C++ compilation is required by Rust tests or CI.
