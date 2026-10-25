# Live base-date CPI coupons and legs

Python and Go can derive the CPI denominator from a retained inflation index
instead of requiring a fixed CPI number. These are **additive named factories**;
existing explicit-base constructors and their numerical behavior are unchanged.

| Layer | Coupon | Leg |
| --- | --- | --- |
| Rust | `CpiCoupon::from_base_date` | `CpiLeg::from_base_date` |
| Python | `CPICoupon.from_base_date` | `CPILeg.from_base_date` |
| Go | `Session.NewCPICouponFromBaseDate` | `Session.NewCPILegFromBaseDate` |
| C | `itofin_cpi_coupon_from_base_date` | `itofin_cpi_leg_from_base_date` |

## Observation contract

- Base CPI is read **lazily and live**, not frozen when the object is built.
  A missing base fixing does not prevent construction, but a valuation requiring
  it fails. Adding a missing fixing or relinking the inflation curve is observed
  by existing coupons and legs.
- A coupon reads its denominator with the native convention:
  `laggedFixing(base_date + observation_lag, observation_lag, interpolation)`.
- The terminal indexed principal reads its denominator with
  `laggedFixing(base_date, zero_lag, interpolation)`. This is intentionally
  different from the coupon route.
- Month arithmetic is not invertible. January 31 plus one month clamps to
  February's end, and subtracting one month can return a different January day.
  Linear interpolation also weights by the unlagged observation period. Do not
  replace either native route with a hand-computed fixed denominator.
- Omitting a leg's base date uses schedule start minus observation lag. An
  omitted base date requires at least one coupon period; an explicit date can
  also construct a single terminal payment in Rust/C.
- Zero-rate leg coupons remain fixed zero coupons requiring no CPI fixings.
  Their terminal indexed principal is still fixing-dependent.

Coupon `base_fixing()` / `BaseFixing()` returns the effective denominator and
can fail. C uses query field 6. The old Rust `base_cpi()` stored-value getter
is unchanged for explicit-base objects; on new date-based coupons it returns
native `f64::MAX`, the **no-explicit-base sentinel**, not an actual CPI level.
Python and Go expose the fallible effective query rather than that sentinel.

The new Go/C factories reuse the old configuration structures and **ignore
`BaseCPI` / `base_cpi`** in this named path, including non-finite stored values.
Old explicit-base factories still validate and use that field. Other finite
parameters, enums, dates, lags and observation bounds remain validated. A
nonzero supplied base date must be valid; only a missing leg base date uses
its default. A zero observed CPI denominator is an error, not a finite price.

## Ownership and pricing

Indices, their Settings and linked curves remain retained and live. Python/Go
leg facades fix payment and ex-coupon dates at construction; `build()` / `Build()`
clones that graph. Rebuild the factory to adopt later calendar holiday edits.
Generic Leg NPV uses the **same Settings object** as the inflation index, not
just equal evaluation dates. Go inputs must also share one Session.

Attach a plain `CPICouponPricer` before querying a directly constructed coupon's
rate or amount. Leg construction attaches the existing plain pricer. This
adds no convexity correction, cap/floor optionlet, new volatility model,
CPIBond base-date constructor or CPISwap fair fixed-rate formula.

## Validation and examples

The tracked coupon matrix covers 36 native cases and three market states:
Flat/Linear, 0/1/3-month lags, signed/zero rates, historical/forecast observations,
ex-coupon accrual and curve relinking. A separate 32-case leg matrix covers
explicit/default bases, irregular schedules, zero-rate coupons, repeating rate
vectors, varying notionals, adjusted principal, growth/full principal and NPV.

Coupon fixtures are produced by `scripts/generate_cpi_base_date_oracle.py`
using QuantLib 1.43. The leg producer uses pinned source headers with the
independent wheel and records that mixed provenance, not a full-source build.
Its entire numerical matrix was also independently reconstructed through SWIG.
See the two provenance JSON files under `sdk/go/testdata/cpi-base-date-*`.

- [Python example](https://github.com/benbenbang/libitofin/blob/main/example/python/cpi_base_date.py)
- [Go example](https://github.com/benbenbang/libitofin/blob/main/sdk/go/examples/cpi_base_date/main.go)
- [Existing plain CPI contract](cpi-cashflow-bindings.md)
