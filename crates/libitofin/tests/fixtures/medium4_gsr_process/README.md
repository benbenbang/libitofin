# Constant GSR transition fixture provenance

- Oracle source commit: `9863b578af0caa4cecabf697196533e84a8308b6`.
- Oracle sources: `QuantLib/ql/processes/gsrprocess.cpp` and
  `QuantLib/ql/processes/gsrprocesscore.cpp` in the parent checkout.
- `generate.py` uses Python's standard-library Decimal with 110 significant
  decimal digits and evaluates the original, unreduced constant-regime risk-neutral
  and forward-measure expectation expressions independently of the Rust algebra.
- Inputs are exact decimal strings. The TSV stores correctly rounded binary64
  values from those high-precision expressions. Tests allow 2e-13 relative error
  for decimal-to-binary input representation and floating-point arithmetic.
- QuantLib treats `abs(a)<1e-4` as zero in several integrals. The small signed
  reversion fixtures instead follow the untruncated mathematical dynamics.
- Regenerate: `python3 generate.py` in this directory. No external packages,
  network, native library or time-dependent input is required.

Source SHA-256:

| File | SHA-256 |
| --- | --- |
| `gsrprocess.cpp` | `4e0db0a05ca121e9d6b2aad08ad83c5b04e5537e8cb172b11ae56290d3b3daf6` |
| `gsrprocesscore.cpp` | `76b7351db8583f0a603b60dd4d76eef8919beaba074863b0e5b4f9e9a2588b0c` |
