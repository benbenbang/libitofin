# Bond risk fixtures

`bond_risk_oracle.tsv` and its identical Python fixture are fresh queries to a
separately installed **QuantLib 1.44 wheel**, not values copied from product
outputs or an upstream test. The 55 construction inputs come from the existing
`bond_analytics_oracle.json`, SHA-256
`ce5c9904d3f5a9e3f2dd10f68e52462a6b7ec0ec4acb0b174d89a998f2660dd3`.

Reproduce from the repository root with an interpreter containing QuantLib 1.44:

```sh
python sdk/go/testdata/generate_bond_risk_oracle.py
```

The generator rejects other runtime versions. Every numeric field is written
with round-trip binary64 representation, with `NA` only for unsupported Macaulay.
It queries all six metrics independently. Unsupported Macaulay queries are
represented by null only for non-Compounded conventions. Fixed and ex-coupon
bonds have a semiannual Backward schedule, NullCalendar, Actual360, coupons
0.047/0.058, redemption 102 and five settlement days. Ex-coupon periods are seven
calendar days. Supplied settlement dates are actual dates, not evaluation dates.

## Separate binary and source provenance

- Wheel version: 1.44, CPython 3.13.15, macOS arm64 (1.43 before #1405).
- Observed `_QuantLib.abi3.so` SHA-256:
  `59239903c2bd3f831040ebdad4f8a6870c82ec482e370e18c2f48f7097c0dd3e`.
- Source **review reference**, not a claim about the compiled wheel revision:
  tag `v1.44` (`ab4cb476d35f7275834bc05b47d1324403d1823b`).
- `ql/pricingengines/bond/bondfunctions.cpp` SHA-256:
  `55a3f5a9158e304460b4e4ed6acf9c07913751df3a1528589164a7eeecb83ae5`.
- `ql/cashflows/cashflows.cpp` SHA-256:
  `58677c9bfe192169c35b74d90696c711619c69b077b357725d6b2ed0137a6452`.
- QuantLib `LICENSE.TXT` SHA-256:
  `58e98bc18648e99865665733f4ed87404a8f1fc757885d119486dec201dddf99`.

Matching `crates/libitofin/THIRD_PARTY_NOTICES.md` and Python package notices
retain the invoked upstream contributors; the complete QuantLib license ships
alongside those notices and in native Go packages.

The independent zero-bond derivative tests do not read these fixture outputs.
Their BPV expectation intentionally preserves native convexity divided by 100,
and BPV/YVBP use raw currency values rather than per-100 normalization.
