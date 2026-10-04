# Retained High-priority numerical references

These fixtures support the retained 18 integration scope after issue-board
reconciliation. Excluded finite-difference, Heston and Hull-White swaption
artifacts are archived outside the repository and are not part of this change.

## Provenance and replay

- Numerical oracle: compiled PyPI `QuantLib==1.43`, Python ABI3 wheel.
- Inspected source: local QuantLib revision
  `9863b578af0caa4cecabf697196533e84a8308b6`, version `1.43-dev`.
- The source revision is not the compiled wheel revision. JSON records both
  explicitly; no same-revision source-build claim is made.
- Existing repository QuantLib license and attribution files remain unchanged.

Use an isolated environment with `QuantLib==1.43` installed, then run:

```sh
python crates/libitofin/tests/fixtures/high28/generate.py
python crates/libitofin/tests/fixtures/high28/generate_callable_edges.py
python crates/libitofin/tests/fixtures/high28/generate_rates.py
python crates/libitofin/tests/fixtures/high28/generate_foundations.py
```

The generator fails on a different QuantLib version. Baked Rust comparisons
require no installed QuantLib. Match dates, compoundings, day counts, settlement
rules, notionals and market inputs before comparing production outputs.

| Fixture | Coverage |
| --- | --- |
| `fx-delta.json` | Call/put, four delta conventions, ATM conventions and failures |
| `cashflow-spreads.json` | Five compoundings, both spread signs, settlement-flow inclusion and inversion |
| `bond-quotes.json` | Ex-coupon accrual and current-notional amortization normalization |
| `callable-prices.json` | Call/put, clean/dirty off-coupon quotes, near-coupon snapping |
| `callable-edge-cases.json` | Ex-coupon settlement, multiple call/put events, maturity call ordering |
| `rates-swaptions.json` | Black/shifted Black/normal physical and cash quote targets |
| `rates-coupons.json` | USD holiday value dates, zero-lag fixing adjustments and arrears |
| `rates-stub-schedules.json` | Forward/backward rules with independent annual/semiannual stub dates |
| `foundations-covariance.json` | Deterministic near-symmetric correlation and standard-deviation conversion |

Generated fixtures establish independent reference values, not a production
parity claim. Production workers own comparison tests and their tolerances.
