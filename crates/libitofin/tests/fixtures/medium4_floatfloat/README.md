# Plain FloatFloatSwap native oracle

- Source audit: repository QuantLib `ql/instruments/floatfloatswap.{hpp,cpp}`,
  original revision `9863b578af0caa4cecabf697196533e84a8308b6`.
- Compiled independent oracle: Python QuantLib 1.43.
- `generate.py` constructs the actual native FloatFloatSwap and
  DiscountingSwapEngine. No Rust helper or rewritten valuation formula is used.
- CI reads tracked `native.csv`; no native QuantLib installation is required.

Reproduce in a Python environment with QuantLib 1.43:

```sh
python generate.py
```

Ten cases cover positive/negative rates and spreads, 3M versus 6M schedules,
Actual360 versus Actual365Fixed coupon accrual, shifted NPV dates, historical
fixings, included/excluded settlement cutoffs, today's Settings override,
explicit Following versus schedule-default Unadjusted payments over weekends,
and expiry. Both native fair spreads reprice in Rust.

Paid leg is negative, received leg positive, and neither has principal flows.
Absolute tolerances are `2e-9` for PV/BPS and `2e-13` for fair spreads.
Expired fair spreads are undefined and deliberately not compared to fixture
placeholder zeros; the Rust API must return errors for zero sensitivity.

`support.rs` constructs the Rust market inputs only. It is not an oracle.
The fixture validates the plain common-notional unit-gearing Ibor slice, not
full native structured FloatFloatSwap parity.
