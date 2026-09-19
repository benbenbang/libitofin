# Overnight averaging oracle

Generated with the independent QuantLib 1.43 Python wheel, using default coupon
pricers (zero volatility, exact arithmetic averaging). From the repository root:

```sh
uv run --with QuantLib==1.43 crates/libitofin/tests/fixtures/overnight_averaging/generator.py
```

- `coupons.csv`: complete coupon inputs and Simple/Compound results. Fixings and
  accrued amounts are `date:value` pairs separated by `|`. Empty numeric fields
  indicate a missing fixing error. No values come from libitofin.
- `ois.json`: one-node OIS helper discount, implied quote and dates, including
  quote/evaluation updates, forward start, payment lag and separate discounting.
- `swaps.json`: payer OIS valuation with separate flat forecast/discount curves,
  a forecast-rate update, mixed fixings and missing-history rejection.
- `bindings.json`: today-start OIS and additive-spread helper repricing through
  the existing Python/Go APIs, which do not expose coupon daily-spread settings.

All indices are ESTR (TARGET, Actual/360). Flat curves use continuously compounded
Actual/365 Fixed rates. Helper curves use Actual/360 log-linear discount factors.
The baseline helper values reproduce `sdk/go/testdata/rates_completion_oracle.json`.
Keep helper discount tolerance at `1e-12` and quote tolerance at `1e-9`.
The external-discount case has three years of semiannual payments; its recorded
endogenous control differs beyond `1e-12` for both modes. Discounting one annual
payment would cancel and could not validate use of the separate curve.

QuantLib's exact Simple pricer uses the full forecast interval for partial accrual
inside an overnight span, including a weekend; historical spans are clipped.
Simple divides by the coupon day counter and ignores the daily-compounded-spread
flag. With today's fixing absent and enforcement enabled, Simple raises a missing
fixing error while Compound forecasts. CSV errors normalize QuantLib's dated
`Missing ESTRON Actual/360 fixing` message to `missing_today` or `missing_past`.
Updated cases specify reproducible target values; consumers should also mutate
retained objects to verify invalidation, rather than only reconstructing them.

The Rust fixture test checks all 18 Simple and 18 Compound rows, including the
[#1045](https://github.com/benbenbang/libitofin/issues/1045) regressions. Additional
cases cover weekend boundaries, forecast/historical daily spread, and enforced
missing-today errors on partial spans. Effective spread and index fixing are
independent QuantLib outputs; Simple has no effective index fixing.

Compound projects complete future intervals with a discount ratio, including
when daily-spread compounding is enabled. Partial forecast boundary intervals
still use individual fixings, apply daily spread and enforce today's fixing
setting. Thus a fully forecast coupon can have zero effective daily spread,
while historical and partial intervals retain it. With daily spread, QuantLib's
rate is `gearing * (effective_index_fixing + effective_spread)`; otherwise it is
`gearing * effective_index_fixing + spread`.
