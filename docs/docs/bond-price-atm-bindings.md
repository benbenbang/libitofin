# Price-fed bond ATM rate

The named price-fed helper is additive. Existing `atm_rate_from_curve` /
`ATMRateFromCurve` still use the bond's own curve NPV, without a price argument.

| Layer | New API |
| --- | --- |
| Rust | `BondFunctions::atm_rate_from_price(bond, curve, BondPrice, settlement)` |
| C | `itofin_bond_atm_rate_from_price(ctx, bond, curve, amount, price_kind, settlement, out, error)` |
| Go | `bond.ATMRateFromPrice(curve, amount, priceType, settlement)` |
| Python | `bond.atm_rate_from_price(curve, amount, price_type=BondPriceType.Clean, settlement=...)` |

## Quote units and numerical contract

- The quote is per 100 of **current settlement notional**, not initial face.
- A clean quote includes the bond's accrued amount, which can be negative in an
  ex-coupon window. Dirty quotes are used directly.
- The dirty quote is divided by 100 and multiplied by current notional. This is
  the target NPV at settlement supplied to the existing cash-flow ATM kernel.
- The remaining principal/non-coupon cashflows are retained. The result is a
  uniform coupon rate, not a discount yield or an instrument pricing-engine NPV.
- Finite negative quotes/rates remain signed, not clamped. Nonfinite quotes,
  intermediate target or output values fail.
- A zero coupon-value target returns zero, even with zero BPS. Otherwise zero BPS
  is an error. The kernel's zero check is exact, not tolerance-based. A
  zero-coupon bond is not unconditionally zero for this new helper.

## Dates, fixings and lifecycle

Explicit settlement is an actual date, without an additional settlement lag.
Omitted settlement requires the existing checked evaluation-date path. Settlement
at or beyond full redemption is nontradable. Settlement-date and ex-coupon flows
are excluded by the inherited cash-flow calculation.

Both clean and dirty queries still read live coupon amounts through the existing
kernel. Missing required fixings and coupon-pricer errors therefore propagate
also for a dirty target. Clean conversion additionally requires accrued-coupon
fixings, including when that coupon trades ex-coupon. Dirty target algebra may
be independent of forward coupon amounts, but that does not bypass these guards.

No engine is required, and a frozen engine/cache is not recalculated or replaced
by the query. Current curve/coupon/settings policies remain observed without
rewriting cached pricing results. Ordinary noncallable fixed, zero and existing
floating bonds are supported; no new instrument constructor, fitted model or
cash-flow algorithm is supplied by this helper.

Go objects/curves must be live and from the same Session; Close/error/thread rules
are unchanged. Bonds retain their existing settings/cashflow dependencies.
Python retains its existing native graph and has no Go Session model. C preflights
output/error alignment and writes the scalar only after successful finite
calculation; callers retain pointer validity/non-overlap obligations.

## Examples

- Python: `example/python/bond_price_atm.py`.
- Go: `sdk/go/examples/bond-price-atm/main.go`.

## Provenance

The price-to-NPV boundary adapts the explicit-price branch of QuantLib
`ql/pricingengines/bond/bondfunctions.cpp`, revision
`9863b578af0caa4cecabf697196533e84a8308b6`, and delegates the existing Rust
`CashFlows::atm_rate`. The prior own-NPV overload and core kernel are unchanged.
Independent numerical fixtures distinguish the separately compiled QuantLib
1.43 distribution from that source revision. This bounded helper is not all
BondFunctions overloads or complete QuantLib parity.
