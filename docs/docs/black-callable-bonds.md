# Black callable bond pricing

`BlackCallableFixedRateBondEngine` prices one European hard issuer call or holder
put on `CallableFixedRateBond` or `CallableZeroCouponBond`.

| Input | Meaning |
| --- | --- |
| Yield-volatility quote | Constant relative forward-yield volatility |
| Discount curve | Cash-flow discounting and forward-yield calculation |
| Settings | Explicit evaluation date for the volatility clock |
| Exercise schedule | Exactly one original and still-active right |

## Calculation

- The engine converts the forward compounded yield to price volatility using
  modified duration. Coupon frequency comes from the product. Zero-coupon
  once/no-frequency conventions use annual compounding.
- The option clock is Actual/365 Fixed from the evaluation date, not necessarily
  the curve reference date.
- Clean exercise prices include indenture accrued interest. The resulting
  currency strike is compared with the forward dirty bond price.
- An issuer call subtracts the embedded call value. A holder put adds the
  embedded put value. Both NPV and settlement value are supplied.
- Quote updates, curve relinking and evaluation-date changes invalidate the
  attached bond through its existing observable and lazy-cache contracts.

## Corrected native conventions

The reference is QuantLib's `experimental/callablebonds/blackcallablebondengine`.
Two demonstrated corrections avoid copying native currency/ownership defects:

1. After subtracting interim coupon income from settlement PV, multiply by the
   settlement discount factor before dividing by the exercise discount factor.
   Both numerator and denominator then use the same curve reference date.
2. Do not subtract interim coupons that trade ex-coupon at settlement. The
   settlement PV already excludes those coupons, so subtracting them again
   understates the forward price.

When settlement equals the curve reference date and there is no excluded
interim income, results match the native Black engine. Positive settlement lags
and excluded ex-coupon income instead use independently discounted remaining
cash flows and native Black formula/yield/duration analytics as the oracle.
Native raw outputs are retained separately in the wave evidence.

## Boundaries and errors

- Multiple original exercise rights are not accepted, even if filtering expired
  rights leaves just one. Empty and expired exercise schedules are also rejected.
- The existing settlement filter excludes exercise on the settlement date.
- Exercise at redemption has no positive residual forward bond and is rejected.
- Exactly zero forward yield that exactly reprices the original cash-flow leg
  uses intrinsic value, without tolerance-based clamping.
- Negative forward yield with positive relative volatility is outside this
  lognormal model. Invalid/nonfinite quotes, cash flows, forwards, discount
  factors, standard deviations or values return errors.
- A failed calculation clears the engine's previous results.
- No volatility surface, soft-call schedule, Bermudan approximation or bindings
  are introduced. Use the existing Hull-White tree for multiple hard rights.

`BlackCallableBondArguments` is an additive engine-specific bundle. The original
public `CallableBondArguments` fields and `PricingEngine` required methods are
unchanged. Existing struct literals and downstream engines remain compatible.
