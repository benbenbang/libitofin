# BondForward compiled oracle

- Inspected QuantLib source: `9863b578af0caa4cecabf697196533e84a8308b6`.
- Numerical reference: independently installed PyPI `QuantLib==1.43`.
- These are distinct revisions, not a claim that the wheel was built from the
  inspected checkout. The output records source hashes and compiled version.
- No Rust implementation, fork implementation or calculated Rust output is used
  by the generator. It constructs and prices native `QuantLib.BondForward`.

Run from any directory with Python and `QuantLib==1.43` installed:

```sh
python generate.py --source-root /path/to/QuantLib --output /tmp/bond-forward.json
```

`medium_bond_forward.rs` bakes the resulting 15 cases, comparing fair, clean and
NPV with an absolute `2e-11` tolerance. JSON is an audit artifact, not a runtime
test dependency. Dates use Actual365Fixed, NullCalendar, a four-coupon bond,
2-day bond settlement, and distinct spot/financing/income curves. The source
paths in the generated provenance cover both forward implementations and the
cash-flow occurrence rule.

The non-100/amortizing and ex-coupon observations deliberately preserve native
mixed-unit/raw-income conventions. Tests also cover checked deviations from
native expired-price and invalid-date behavior. Future-dated value-date floors
exclude income on settlement; delivery-date coupons and redemption are included.
