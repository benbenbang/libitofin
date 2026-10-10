# Plain cross-currency basis swaps

`instruments::XccyBasisSwap` is an additive Rust-core value-style instrument:

| Contract | Behavior |
| --- | --- |
| Orientation | Pay base Ibor coupons/principal, receive quote Ibor plus basis |
| Initial exchange | Receive base principal, pay quote principal |
| Notionals | Independent positive constant amounts, not forced to match spot |
| FX direction | `ExchangeRate` source/base to target/quote, quote units per base |
| Discounting | Each leg uses its currency curve; both reference dates must match |
| Forecasting | Each Ibor index retains its forwarding handle and fixing history |
| Basis | Added only to quote coupons, not to principal |
| Exchanges | Mandatory initial and final principal, no intermediate resets |

## Dates and FX snapshot

- Default settlement cutoff and common NPV date are the current evaluation date.
- `with_valuation_dates(settlement, npv_date, include)` supplies fixed dates.
- Cashflow filtering follows `CashFlows` and Settings. Today's explicit cashflow
  override takes precedence over the inclusion flag when cutoff equals today.
- Both legs use the earliest coupon accrual start and latest coupon accrual
  maturity across both schedules for exchanges. Each exchange is adjusted on
  its own schedule calendar using its schedule convention. This is not the
  last coupon payment date when payment adjustment moves that coupon.
- Coupon payment adjustment defaults to Following; the constructor can override
  it. Coupon accrual day counters are explicit.
- Spot is an immutable snapshot at the common discount reference date by
  default. `with_spot_settlement_date(date)` interprets it at that later date.
- Conversion uses the native-engine parity adjustment:
  `effective_spot = spot * D_quote(spot_date) / D_base(spot_date)`.
- NPV and settlement dates must not precede the common curve reference date;
  spot settlement cannot precede it either. Unequal discount reference dates
  fail explicitly, including after a relink.

The value is `Q / effective_spot - B` in base currency and
`Q - effective_spot * B` in quote currency. `B` and `Q` include unsigned
contractual coupon legs with negative initial and positive final exchanges;
`base_leg_pv` is not already multiplied by the paid-leg sign.

`fair_quote_basis_spread` returns the absolute quote spread, not a shift:

```text
fair = current_spread - quote_currency_NPV / (quote_leg_BPS / 0.0001)
```

It errors when no surviving coupons produce a nonzero spread sensitivity.
Expired cashflows price to zero, but empty/out-of-domain discount handles still
error because this value-style API validates the market before valuation.

## Validation and live behavior

- Spot/index currencies must match in the documented order and currencies
  must differ. Blank currencies, invalid dates, unordered schedules, non-finite
  rates/spreads and non-positive notionals/spot are rejected.
- Day-tenor indexes, including erased overnight indexes, are rejected rather
  than silently priced as simple Ibor coupons.
- Both indexes must share the swap's `Settings`. Missing required fixings
  propagate errors rather than silently forecasting.
- Each query rereads forwarding and discount handles and observable curve
  quotes. No observer-cache invalidation is needed.
- Spot itself is not observable: rebuild with a refreshed snapshot/date when
  the spot market changes. Yield curves contain no currency metadata, so
  callers must supply the proper currency discount curves.

## Source, compatibility and boundaries

The original QuantLib sources at
`9863b578af0caa4cecabf697196533e84a8308b6` were independently inspected. Baked
fixtures come from compiled PyPI QuantLib 1.43's actual
`ConstNotionalCrossCurrencyBasisSwap` and discounting engine, which is a
separate revision. They cover asymmetric schedules, arbitrary notionals,
off-reference NPV/FX-settlement dates, cutoff inclusion, historical fixings,
weekend maturity and fair-spread repricing. CI needs no native installation.

This is not complete parity with that native class family. The API retains
explicit Ibor day counters and a snapshot FX value rather than native engine
inheritance and live FX handles. Compared with the historical fork proposal,
currencies/notionals are validated, exchanges use common native accrual bounds,
discount reference dates must agree, FX settlement is explicit, and zero spread
sensitivity returns an error. No existing repository API or default is changed.

Resettable/MtM variants, overnight coupon extensions, optional principal
exchanges, payment lags, non-unit gearings and base-leg spreads are deferred.
No C, Go or Python facade is introduced.
