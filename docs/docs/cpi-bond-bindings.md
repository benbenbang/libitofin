# CPI bonds

`CpiBond` in Rust, `CPIBond` in Python and `Session.NewCPIBond` in Go construct
plain CPI-indexed coupon bonds. The C ABI entry point is `itofin_cpi_bond_new`.
This is the modern, explicit-base-CPI construction path, not the deprecated
`growthOnly` overload.

## Cash flows and price units

- Each coupon pays face amount times its fixed annual coupon rate, accrual
  fraction and observed CPI divided by the explicit base CPI.
- Terminal principal pays the full face amount times observed CPI/base CPI.
  It is a live, fixing-dependent cash flow, not an ordinary fixed redemption.
- CPI observations use the unadjusted accrual end or schedule maturity with
  the supplied observation lag. Flat and Linear conventions are supported.
- Accrued interest observes CPI at the requested accrual date. Ex-coupon
  accrued interest can be negative.
- Dirty/clean prices and accrued interest are per 100 of original outstanding
  face amount. They are not divided by inflation-adjusted principal.
- Contractual maturity is schedule end. The final payment may be adjusted to
  a different date. Outstanding notional is the face amount strictly before
  the adjusted terminal payment day, and zero on that day and afterwards,
  regardless of `includeReferenceDateEvents`. Instrument expiry can still
  differ on that payment day when the event-inclusion setting changes.

## Inputs and retained dependencies

Provide a positive finite face amount and base CPI, an explicit inflation index,
observation lag, interpolation convention, increasing schedule of at least two
valid dates, nonempty finite coupon-rate vector, accrual day counter and the
same Settings used by the index. Short coupon vectors repeat their final rate.

Payment convention defaults to Modified Following. The payment calendar defaults
to the schedule calendar. Optional issue date and ex-coupon conventions follow
the common bond interface. The bounded constructor does not expose growth-only
principal, amortization, capped/floored CPI coupons, a separate first-period day
counter, or a one-date zero-coupon schedule.

Use the existing `DiscountingBondEngine` with the same Settings. CPI forecasting
uses the inflation curve linked to the index; discounting uses the engine's
nominal curve. The instrument retains both dependency graphs. Quote changes,
index fixing additions, inflation-curve relinks and evaluation-date changes
invalidate pricing through the existing observer graph. Construction is lazy:
missing fixings or a forecast curve are reported when an amount or valuation
actually requires them.

Python objects are thread-confined retained Rust objects. Go objects belong to
one Session, retain native dependencies after wrapper Close, and must be closed
explicitly. Mixed-session inputs are rejected. Different Settings identities
are rejected even when their evaluation dates happen to be equal.

## Supported operations and boundaries

The resulting object exposes Bond valuation, cash-flow inspection, settlement,
notional, flat-yield duration/convexity/BPV and lifecycle operations. The inherited
coupon BPS-from-curve/yield and ATM-rate-from-curve/price methods reject CPI bonds:
the generic coupon weights omit the CPI ratio and must not be interpreted as
sensitivity to the raw contractual CPI coupon rate. No core formula is changed. The native proof in
`sdk/go/testdata/cpi-bond-coupon-risk.json`, reproduced by
`scripts/generate_cpi_bond_coupon_risk_oracle.py`, contrasts generic BPS with an
actual one-basis-point contractual-rate bump and generic ATM with the raw rate. Date-only inspection
must not evaluate CPI amounts. Retained cash-flow and redemption wrappers remain
index-dependent, including after the original bond wrapper is dropped or closed.
Extracted CPI legs retain the original Settings identity for NPV and spread-NPV
queries, reject mixed Settings, and reject nonfinite aggregate results.

The existing bounded `AssetSwap` and `BondForward` facades continue to accept
only their documented fixed/zero plain-bond scope. They reject CPI bonds before
trying to resolve CPI fixings. This release does not extend those consumers to
indexed bonds or add CPI optionality to the bond.

All additions preserve existing bond constructors and signatures.

## Numerical reference

Tests use independently generated QuantLib 1.43 wheel results in
`sdk/go/testdata/cpi-oracle-bonds.json`, with shared inputs and provenance in
`cpi-oracle-inputs.json`. Reproduction is in `scripts/generate_cpi_oracle.py`.
The supplemental `cpi-bond-preceding.json` fixture and
`scripts/generate_cpi_bond_preceding_oracle.py` cover adjusted principal payment
before contractual maturity, terminal notional shutdown and event inclusion.
The source reference is QuantLib revision
`9863b578af0caa4cecabf697196533e84a8308b6`,
`ql/instruments/bonds/cpibond.{hpp,cpp}`. The wheel is not claimed to have been
built from that source revision.

Coverage includes Flat/Linear observations, per-100 clean/dirty/accrued units,
full indexed principal, live discount changes, inflation relinks, historical
fixing changes, adjusted final payment dates, negative ex-coupon accrual and
missing-fixing errors. This bounded comparison is not a full QuantLib parity
claim.
