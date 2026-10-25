# CPI versus Ibor swaps

`CpiSwap` in Rust, `CPISwap` in Python and Go, and the C
`itofin_cpi_swap_*` functions exchange CPI-scaled fixed coupons against plain
Ibor-plus-spread coupons. This is not `ZeroCouponInflationSwap`.

## Supported contract

- `Payer` **pays Ibor and receives CPI**. `Receiver` reverses both signs,
  matching QuantLib's CPISwap convention rather than fixed-leg payer intuition.
- Both schedules contain at least two increasing dates. Day counters and
  payment conventions are explicit, independently specified for each leg.
- Ibor fixing days use the index default. Day-tenor/overnight, in-arrears,
  geared, capped and floored coupons are not included.
- The supplied positive base CPI scales every CPI coupon and terminal flow.
  Observation lag and Flat/Linear interpolation follow the CPI foundation.
- CPI principal depends on inflation fixings even though it is not a coupon.
  Missing fixings and unavailable forecasts are errors, never zero payments.
- The constant inflation nominal defaults to the Ibor nominal in Python/Go;
  both are explicit in Rust/C. They can differ.

## Terminal exchanges

There is no initial exchange. Without nominal subtraction, the CPI leg pays
`inflation_nominal * CPI_ratio` and the Ibor leg pays `nominal` at their
respective final payment dates.

With subtraction enabled, CPI principal is
`inflation_nominal * (CPI_ratio - 1)`. Ibor principal becomes
`nominal - inflation_nominal`; matching the native convention, that Ibor flow
is omitted when the difference has absolute value at most `1e-5`.
Coupon rates are unaffected by this choice.

## Pricing and lifecycle

Attach a live nominal discount curve with `set_engine` / `SetEngine` and the
same Settings allocation used by both indexes and the swap. Existing generic
swap discounting uses its default settlement and NPV dates.

Queries include total NPV, signed CPI-leg and Ibor-leg NPVs, signed Ibor spread
BPS, and the absolute fair Ibor spread. Terminal principal is included in NPV,
not in spread BPS. `fair_spread` / `FairSpread` holds the entire CPI leg fixed;
zero spread sensitivity is an error.

A fair CPI fixed-rate query is deliberately not exposed. QuantLib's generic
fallback uses unscaled coupon BPS and is not generally a true par fixed rate
when CPI ratios differ from one.

Indexes, Settings, schedules and engine dependencies are retained after their
facade handles are released. Curve relinks, quotes, fixings and evaluation-date
changes invalidate native pricing. Freeze/unfreeze and forced recalculation
retain the existing lazy-instrument behavior. Go calls use the Session worker;
objects from different sessions cannot be combined.

This additive slice does not change existing inflation or swap APIs, add CPI
volatility models, or establish full QuantLib product parity.

## Reference

Construction and exchange conventions follow QuantLib revision
`9863b578af0caa4cecabf697196533e84a8308b6`,
[`ql/instruments/cpiswap.cpp`](https://github.com/lballabio/QuantLib/blob/9863b578af0caa4cecabf697196533e84a8308b6/ql/instruments/cpiswap.cpp).
Independent numerical fixtures are generated with the separately identified
QuantLib 1.43 wheel, not asserted to be compiled from that source revision.
