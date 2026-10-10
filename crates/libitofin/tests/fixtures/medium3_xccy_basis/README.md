# Plain cross-currency basis swap oracle

- Original source audit: QuantLib revision `9863b578af0caa4cecabf697196533e84a8308b6`.
- Compiled independent oracle: PyPI QuantLib 1.43, a different revision.
- Generator imports only QuantLib and pathlib, never libitofin or the Rust code.
- `native.csv` is baked into Rust tests; CI does not need installed QuantLib.

Reproduce with a Python environment containing QuantLib 1.43:

```sh
python generate.py
```

## Native oracle coverage

The generator constructs the actual `ConstNotionalCrossCurrencyBasisSwap` and
`DiscountingConstNotionalCrossCurrencySwapEngine`, not a rewritten valuation
formula. Each case reprices through separate domestic-base and domestic-quote
engines. `inCcyLegNPV` supplies local-currency PVs, `inCcyLegBPS` supplies quote
spread sensitivity, and `fairRecSpread` supplies the fair quote spread.

- Matched and unequal notionals, positive and negative basis spreads.
- Asymmetric accrual starts, maturities and numbers of coupons.
- Explicit non-reference NPV and spot-settlement dates.
- Included/excluded cutoff cashflows, today-cashflow Settings override.
- Required historical fixings, expired legs, updated forward/discount rates.
- WeekendsOnly quote schedule with Unadjusted accrual/principal maturity on
  Sunday and Following coupon payment on Monday. This detects using coupon
  payment date instead of native coupon accrual maturity for exchanges.

Rust PV/BPS tolerance is absolute `2e-9`, fair-spread tolerance `2e-13`.
Separate tests check quote updates, curve relinks, missing histories, malformed
contracts and exact notional amounts/dates. Expired fair spread is undefined and
must return an error; its fixture zero is not asserted as a native fair spread.

## Source contracts audited

- `ql/instruments/constnotionalcrosscurrencybasisswap.cpp`: Ibor legs, common
  earliest accrual start/latest coupon maturity, fair-receive-spread sign.
- `ql/instruments/constnotionalcrosscurrencyswap.cpp`: per-calendar exchanges.
- `ql/cashflows/cashflows.cpp`: maturity of coupons uses accrual-end date.
- `ql/pricingengines/swap/discountingconstnotionalcrosscurrencyswapengine.cpp`:
  equal discount reference dates, explicit cutoff/NPV date, spot-settlement FX
  parity adjustment and local-versus-converted leg results.

This validates the plain Ibor slice only: no resettable/MtM notionals, overnight
coupon conventions, general native inheritance/engine interface or live spot.
