# Callable bond implied volatility

`CallableFixedRateBond::implied_volatility` and
`CallableZeroCouponBond::implied_volatility` recover the constant forward-yield
volatility that matches a bond's settlement quote.

| Input | Convention |
| --- | --- |
| `target_price` | `BondPrice::Clean` or `BondPrice::Dirty`, per 100 of face |
| `discount_curve` | Curve used by the Black callable engine |
| `accuracy` | Positive finite volatility-root accuracy |
| `max_evaluations` | Strict positive pricing-evaluation budget |
| `min_vol`, `max_vol` | Finite bracket, `0 <= min_vol < max_vol` |

## Supported contracts

- Exactly one future hard European issuer call or holder put.
- Fixed-rate bonds and zero-coupon bonds.
- Constant forward-yield volatility, not Hull-White short-rate volatility.
- Forward-yield restrictions follow the Black callable engine.
- No bindings, volatility surface, multiple exercise dates or calibration model.

The method returns an error for an expired bond, invalid inputs, a target outside
its volatility bracket, an exhausted evaluation budget or a Black pricing failure.
An exact zero-volatility or upper-bound target returns that endpoint. If prices
are numerically insensitive to volatility, an endpoint solution does not establish
that implied volatility is unique.

## Settlement and lifecycle

Clean prices are converted to dirty prices by adding settlement accrued interest.
The target currency value is `dirty_price / 100 * face_amount`; it is matched to
the engine's **settlement value**, not its curve-reference NPV. Solver residuals
are normalized to quoted units to avoid face-size-dependent zero tests. Zero-coupon clean
and dirty prices coincide.

Inversion uses a temporary engine and quote. It does not require an installed
engine and does not replace one, populate or invalidate the bond's cached results,
change its freeze state or notify its downstream observers. This also holds on
pricing and solver errors.

## Oracle boundary

Zero-settlement-lag results agree with direct QuantLib 1.43 callable inversion.
At positive lag, the new Black engine corrects the native implementation's missing
settlement discount in its forward price normalization:

`forward = (settlement_value - settlement_income) * discount(settlement) / discount(exercise)`.

Therefore, a raw native positive-lag quote need not imply the same volatility in
this implementation. Tests retain native quotes separately and use independent
QuantLib cash-flow, yield, modified-duration, Black-formula and Brent calculations
for the corrected positive-lag targets. Native defects are not imported simply to
make a fixture agree.

## Choosing a target

Use the same bond terms, exercise convention, settlement date and discount curve
as the Black valuation that produced the quote. A market quote that cannot be
matched within the supplied bracket returns an error; widening the bracket does
not make an unsupported contract valid.

This API is additive. Existing public callable arguments, engine trait requirements
and Hull-White pricing remain unchanged.

An exactly zero forward yield makes pricing volatility-insensitive. A matching
target can return a bracket endpoint; the volatility is not uniquely identified.
