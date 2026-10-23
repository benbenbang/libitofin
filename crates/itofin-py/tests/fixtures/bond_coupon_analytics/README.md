# Ordinary bond coupon BPS and own-NPV ATM oracle

- Producer: independent calls to the separately installed compiled QuantLib 1.43
  Python distribution, not the Rust implementation.
- Local source inspection reference: QuantLib commit
  `9863b578af0caa4cecabf697196533e84a8308b6`,
  `ql/pricingengines/bond/bondfunctions.{hpp,cpp}` and `ql/cashflows/cashflows.cpp`.
  The compiled distribution is not claimed to be built from that checkout.
- Inputs reuse the established `sdk/go/testdata/bond_analytics_oracle.json`
  construction contract: Actual360 coupon/yield day counter, Actual365Fixed
  flat curve at 0.032, five settlement days, NullCalendar, Unadjusted payments,
  redemption 102, fixed coupons 0.047/0.058 and optional seven-day ex-coupon period.
- `scripts/fixtures/bond_coupon_analytics/generate.py` produces the same 55-row tab-separated binary64 observations in
  `crates/itofin-py/tests/fixtures/bond_coupon_analytics.tsv` and
  `sdk/go/testdata/bond_coupon_analytics.tsv`. Both clients read the exact text.
- `bps_curve` and `bps_yield` are coupon-rate sensitivity per 100 of notional.
  `atm_curve` uses the default own-NPV target, not a price/par overload.
- All five compounding modes and four frequencies have nonzero coupon cases.
  Zero-coupon threshold cases deliberately retain zero BPS/ATM behavior; they
  do not independently establish numerical accuracy of a nonzero sensitivity
  around those thresholds.
- Regression comparisons retain absolute 3e-12 and relative 3e-13 tolerances.
  Independent annuity-law, face-invariance, live/frozen-state, amount-free
  missing-fixing BPS, invalid-domain and ownership tests complement the table.

Run the generator with an environment containing QuantLib 1.43 to regenerate.
No external package is required to execute the committed regression tests.
