# MF state binding fixture provenance

`mf_state_native.tsv` and `mf_state_reference.tsv` are exact copies of the existing
Rust core fixtures in `crates/libitofin/tests/fixtures/medium3_mf_state/`.

- Native: 51 observations from the actual original QuantLib C++ process at
  `9863b578af0caa4cecabf697196533e84a8308b6`, not a recreated formula.
- Reference: 15 independently generated 80-digit Decimal observations from
  the source-defined exponential antiderivative; binary64 inputs/endpoints.
- Original source: `ql/processes/mfstateprocess.{hpp,cpp}`, Copyright (C) 2013
  Peter Caspers. QuantLib license: `crates/libitofin/QUANTLIB_LICENSE.txt`.
- Existing fixture README/generator/native client retain original provenance
  and reproduction details. No new native compilation is claimed by copying them.
- Separate QuantLib PyPI1.43 does not export MfStateProcess and generated neither
  file. Native extreme cancellation/underflow artifacts are not Decimal targets.
- Both facades preserve the existing core relative tolerance2e-13 with the same
  minimum-normal floor. Native raw diffusion and empty-grid unit variance remain.
