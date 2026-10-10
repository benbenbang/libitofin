# Zero-coupon bond native vectors

`generate.py` calls the compiled PyPI **QuantLib 1.43** `ZeroCouponBond` and
`DiscountingBondEngine` directly. It writes `native.csv`; the Rust integration
test includes that tracked file and never needs Python or QuantLib in CI.

```sh
python generate.py
```

The independently inspected original source is pinned at
`9863b578af0caa4cecabf697196533e84a8308b6`:

- `ql/instruments/bonds/zerocouponbond.{hpp,cpp}`
- `ql/instruments/bond.{hpp,cpp}`, especially `setSingleRedemption`
- `ql/pricingengines/bond/discountingbondengine.cpp`

The 1.43 wheel is a different revision, not a build of that pinned checkout.
The twelve vectors use a fixed 2026-10-01 curve reference, WeekendsOnly calendar,
Actual/365 Fixed and annual frequency. They cover Following, ModifiedFollowing,
Preceding and Unadjusted; below/at/above-par redemptions; simple, annual compounded
and continuous discounting; negative/zero/positive rates; settlement lags;
settlement before/on/after payment; issue-date settlement floors, including an
issue after payment; zero face and zero redemption; contractual maturity separate
from payment; and expiry. Date columns are day offsets from the curve reference.

Expected prices, cash values, notional and expiry are native method outputs.
The fixed absolute comparison tolerance is **2e-12** for amounts up to 2629.
Additional Rust tests check quote/evaluation-date invalidation, event inclusion,
yield inversion, ownership transfer and errors. Their exponential identities
are independent single-payment source-law checks, not generated native vectors.
Finite input/product rejection intentionally strengthens the new constructor
versus native behavior. It is tested as Rust policy, not claimed as native parity.
