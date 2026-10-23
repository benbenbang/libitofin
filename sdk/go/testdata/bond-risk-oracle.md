# Bond risk fixtures

`bond_risk_oracle.tsv` and its identical Python fixture are fresh queries to a
separately installed **QuantLib 1.43 wheel**, not values copied from product
outputs or an upstream test. The 55 construction inputs come from the existing
`bond_analytics_oracle.json`, SHA-256
`b1a1480e27af119fd55502d31e74a933280a7370b0bf350dce2c80aa4285efff`.

Reproduce from the repository root with an interpreter containing QuantLib 1.43:

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

- Wheel version: 1.43, CPython 3.13.16, macOS arm64.
- Observed `_QuantLib.abi3.so` SHA-256:
  `04a53b6f425e728083149241f77e9b9711db845614ce65d2544a09a0970597d8`.
- Source **review reference**, not a claim about the compiled wheel revision:
  `9863b578af0caa4cecabf697196533e84a8308b6`, version 1.43-dev.
- `ql/pricingengines/bond/bondfunctions.cpp` SHA-256:
  `55a3f5a9158e304460b4e4ed6acf9c07913751df3a1528589164a7eeecb83ae5`.
- `ql/cashflows/cashflows.cpp` SHA-256:
  `a0a1d9fb9d98a79c13e6441f1af8c11faf1b572409b2b113a8b04ba0687cb58c`.
- QuantLib `LICENSE.TXT` SHA-256:
  `58e98bc18648e99865665733f4ed87404a8f1fc757885d119486dec201dddf99`.

Matching `crates/libitofin/THIRD_PARTY_NOTICES.md` and Python package notices
retain the invoked upstream contributors; the complete QuantLib license ships
alongside those notices and in native Go packages.

The independent zero-bond derivative tests do not read these fixture outputs.
Their BPV expectation intentionally preserves native convexity divided by 100,
and BPV/YVBP use raw currency values rather than per-100 normalization.
