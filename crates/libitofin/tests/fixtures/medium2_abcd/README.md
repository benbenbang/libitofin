# ABCD fixture provenance

- Original source contract: QuantLib checkout `9863b578af0caa4cecabf697196533e84a8308b6`,
  `ql/math/abcdmathfunction.{hpp,cpp}` and
  `ql/termstructures/volatility/abcd.{hpp,cpp}`.
- Compiled oracle: official QuantLib Python wheel **1.43**, distinct from that
  source revision. The wheel directly exposes `AbcdMathFunction` and
  `AbcdFunction`; it does **not** expose `AbcdSquared`.
- `generate.py` prints 30 native values, 30 native integrated covariances and
  20 native instantaneous covariances. Arrays are embedded in
  `tests/medium2_abcd.rs`, so tests do not need Python or QuantLib installed.
- `AbcdSquared` is checked against the directly exported native instantaneous
  covariance, the exact operation to which the original helper delegates.
- Three supplemental small-decay constants are computed by Python Decimal
  at 80-digit precision using independent 100-term exponential moment sums.
  They are mathematical references, not compiled native outputs.
- Native covariance is evaluated only in ordinary well-conditioned regimes:
  its primitive formula loses precision at very small decay/intervals and can
  overflow at long maturities. Those regimes use mathematical invariants and
  independent high-precision/quadrature QA, not invented native values.

Reproduce with `python generate.py` in an environment containing QuantLib 1.43.
The printed numbers, before Rust formatting, are deterministic. Source pin and
wheel provenance intentionally are not claimed to be the same revision.
