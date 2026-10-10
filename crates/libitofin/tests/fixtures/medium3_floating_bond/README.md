# FloatingRateBond native oracle

- Independent numerical oracle: compiled PyPI QuantLib **1.43**, run with the
  task-local Python environment. CI reads `oracle.tsv` without Python/QuantLib.
- Original source reviewed independently at QuantLib revision
  `9863b578af0caa4cecabf697196533e84a8308b6`: `floatingratebond.hpp/.cpp`,
  `cashflowvectors.hpp/.cpp`, `iborcoupon.cpp`, `floatingratecoupon.cpp`,
  `capflooredcoupon.cpp`, `bond.cpp` and `couponpricer.cpp`.
- The wheel is a different revision from the original source. Native numerical
  agreement does not establish exhaustive parity with that pinned revision.

Regenerate with `python generate.py` in an environment containing QuantLib 1.43.
The script uses native C++ `FloatingRateBond`, coupons, Black pricer and
`DiscountingBondEngine`. It does not import libitofin or reproduce its formulas.

## Fixture scenarios

| Case | Contract |
| --- | --- |
| plain_par / plain_indexed | Short stubs, broadcast defaults, distinct forecast modes |
| long | Long first/last stubs and source-law reference dates |
| collar / negative | Positive/negative gearing with Black cap and floor |
| sentinel | Native per-entry absent cap/floor sentinel on fixed entries, floating middle collar |
| mixed | Fixed and floating coupons with broadcast collar levels |
| fixed | All-zero gearing, floor-then-cap, even when cap is below floor |
| ex_known | Historical fixing, ex-coupon exclusion and negative accrual |
| holiday | Explicit zero fixing lag with Following on holiday accrual dates |

All cases use TARGET, Actual/360, face 250, redemption 97, settlement lag 2,
forecast 3%, discount 2.5% and caplet volatility 20%. The short schedule has
unadjusted dates 2026-02-01, 2026-05-15, 2026-11-01 and 2027-02-28;
its final payment is adjusted to 2027-03-01 but maturity stays 2027-02-28.
Issue date clamps the initial settlement date to the schedule start.

TSV coupon rows: case, `coupon`, schedule index, payment/start/end/reference
start/reference end/ex-coupon/fixing date serials, fixing days, gearing, spread,
full coupon rate, amount, accrual fraction. Fixed coupon fixing fields are zero.
Price rows: case, `price`, evaluation/settlement/maturity date serials, NPV,
dirty price, clean price, accrued amount per 100, notional, redemption amount.

Tests compare rates at 1e-13, amounts at 1e-11, prices at 1e-10, accrual at
1e-12, accrual fractions at 1e-15 and dates exactly. No existing tolerance changes.
