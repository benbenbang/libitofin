# Bond coupon BPS and ATM rate

These additive helpers expose existing `BondFunctions` analytics on ordinary
fixed-rate, zero-coupon and floating-rate bonds through Python and C/Go.
They do not change core pricing mathematics or existing constructors.

| Python method | Go method | Meaning |
| --- | --- | --- |
| `bps_from_curve(curve, settlement=...)` | `BPSFromCurve(curve, settlement)` | Coupon-rate BPS per 100 with curve discounting |
| `bps_from_yield(yield_rate, dc, compounding, frequency, settlement=...)` | `BPSFromYield(rate, config)` | Coupon-rate BPS per 100 with flat-yield discounting |
| `atm_rate_from_curve(curve, settlement=...)` | `ATMRateFromCurve(curve, settlement)` | Coupon rate repricing the bond's own curve NPV |

## Units and target

BPS means a uniform one-basis-point change in coupon rates, not a one-basis-point
change in discount yield. The value is normalized per 100 of current notional.
For otherwise identical ordinary bullet bonds, changing face amount does not
change these per 100 results. In contrast, `basis_point_value` / `BasisPointValue`
is the existing signed, unscaled currency yield sensitivity.

ATM uses the existing core default target: the bond's own curve-discounted NPV.
It is not a par-price target and has no caller-supplied price overload. With a
single fixed coupon rate, the result recovers that coupon rate. With differing
coupon rates, it is their discounted-accrual-weighted rate. Redemptions are not
coupons and do not create BPS. A zero-coupon bond returns zero for all three
queries, preserving native behavior.

## Dates, ownership and pricing lifecycle

- Explicit settlement is an actual date, not a reference date to advance by the
  bond's settlement lag. Omitted settlement uses the existing checked bond default
  and requires an evaluation date. Nontradable settlement is an error.
- Settlement-date flows and ex-coupon coupons are excluded, preserving core laws.
- BPS reads coupon notionals/accrual periods, not coupon amounts. A floating bond
  can therefore have BPS even when a required historical fixing is unavailable.
  ATM needs coupon amounts and propagates fixing/pricer errors.
- No pricing engine is needed. An attached or frozen engine does not supply the
  analytical answer. Queries use current explicit curve/coupon data without
  rewriting cached instrument results.
- The bond retains its own cashflows/settings and floating dependencies. Go still
  requires the supplied curve/day counter wrappers to be live and from the same
  session. Close a session or object explicitly; no new finalizers are introduced.
- Callable bonds are rejected. These methods do not supply callable effective
  sensitivity, a new pricing engine, all `BondFunctions` overloads or full QuantLib
  parity. The separate existing callback-cycle limitation remains unchanged.

## Examples

Python: `example/python/bond_coupon_analytics.py`.

Go: `sdk/go/examples/bond-coupon-analytics/main.go`.

## Validation and provenance

The shared 55-row binary64 table exercises all five existing compounding modes
and four frequencies plus zero-coupon and ex-coupon cases. It is generated
by independent calls to the separately compiled QuantLib 1.43 distribution using
`scripts/fixtures/bond_coupon_analytics/generate.py`.
This distribution is not claimed to be built from the local QuantLib source
checkout `9863b578`. Public regression tests additionally exercise analytical
annuity laws, units, errors, live inputs, retained dependencies and missing fixings.
