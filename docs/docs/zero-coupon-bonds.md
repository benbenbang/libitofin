# Zero-coupon bonds

`ZeroCouponBond` is an additive Rust-core instrument. It composes the existing
`Bond`, pays no coupons, and creates exactly one redemption cash flow.

| Input | Meaning |
| --- | --- |
| `settlement_days` | Business-day lag, using the bond calendar |
| `calendar` | Settlement and redemption adjustment calendar |
| `face_amount` | Outstanding notional before payment |
| `maturity_date` | Unadjusted contractual maturity |
| `payment_convention` | Adjustment of the redemption payment date |
| `redemption` | Payment percentage, per 100 of face amount |
| `issue_date` | Optional floor on settlement |
| `settings` | Explicit evaluation-date handle |

QuantLib's defaults are Following, redemption 100 and no issue date. Rust makes
these arguments explicit, consistent with the existing fixed-rate constructor.

```rust
use libitofin::instruments::ZeroCouponBond;
use libitofin::settings::Settings;
use libitofin::shared::shared;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendars::WeekendsOnly;
use libitofin::time::date::{Date, Month};

let settings = shared(Settings::new());
settings.set_evaluation_date(Date::new(28, Month::October, 2026));
let zero = ZeroCouponBond::new(
    2, WeekendsOnly::new(), 1_000.0,
    Date::new(31, Month::October, 2026),
    BusinessDayConvention::Following, 100.0, None, settings,
)?;
assert_eq!(zero.bond().cashflows().len(), 1);
# Ok::<(), libitofin::errors::QlError>(())
```

- Contractual maturity remains October 31. Redemption pays November 2.
- `bond()` exposes the existing settlement, notional, accrual and yield APIs.
- `bond_mut()` attaches a `DiscountingBondEngine` through `Instrument::base_mut`
  and exposes lazy clean/dirty prices. `into_bond()` transfers ownership.
- Accrued interest is zero. Clean and dirty prices coincide, quoted per 100.
- Outstanding notional becomes zero on the adjusted redemption date. Thus a
  settlement on payment has zero price even when valuation precedes payment.
- Expiry follows evaluation-date cash-flow inclusion settings, not settlement
  or contractual maturity. NPV and settlement value can therefore differ.
- Issue date floors settlement, even after redemption. The native empty-leg
  constructor does not enforce issue before payment; this wrapper preserves it.
- Face, redemption and `face * redemption / 100` must be finite. Null maturity
  dates return an error before calendar adjustment. Zero amounts are valid. The finite guards
  are stricter than native QuantLib, only for this new constructor.
- No evaluation date gives a settlement error unless an explicit date is used.
  An unexpired bond needs an engine to provide NPV and settlement value.

Validation uses twelve tracked outputs from compiled QuantLib 1.43, separately
from source inspection at revision `9863b578af0caa4cecabf697196533e84a8308b6`.
See `crates/libitofin/tests/fixtures/medium3_zero_coupon/README.md` for provenance
and scope. The Rust constructor contract above is unchanged.

[C, Go and Python fixed/zero bond bindings](bond-bindings.md) now provide
construction, discounting and common bond inspectors. Those public facades
require positive finite face amounts and redemption percentages, while the Rust
constructor continues to accept zero amounts. Floating/callable bindings and
yield/Z-spread analytics are not included in this foundation slice.
