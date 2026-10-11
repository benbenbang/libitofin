# Callable zero-coupon bonds

`CallableZeroCouponBond` adds hard issuer calls and holder puts to a single
redemption. It composes `ZeroCouponBond` and shares the resulting `Bond`'s
instrument base, lazy cache, evaluation-date observer and settlement results.
Existing fixed-rate callable bonds, plain zero-coupon bonds and engines are
unchanged. This is a Rust-core addition, without new bindings or a release.

## Construction and quotes

```rust
use libitofin::instruments::{BondPrice, Callability, CallabilityType, CallableZeroCouponBond};
use libitofin::settings::Settings;
use libitofin::shared::shared;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendars::target::Target;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;

let settings = shared(Settings::new());
settings.set_evaluation_date(Date::new(12, Month::October, 2026));
let call = Callability::new(
    BondPrice::Clean(98.75), CallabilityType::Call,
    Date::new(17, Month::June, 2028),
)?;
let bond = CallableZeroCouponBond::new(
    2, 1000.0, Target::new(), Date::new(12, Month::October, 2031),
    Actual365Fixed::new(), BusinessDayConvention::Following, 107.5,
    Some(Date::new(9, Month::October, 2026)), vec![call], settings,
)?;
```

- Face amount is positive and finite. Redemption is finite and nonnegative,
  expressed per 100 of face. Scaled payment must remain finite.
- Contractual maturity stays unadjusted. The calendar and payment convention
  determine the redemption payment date, which can differ from maturity.
- Issue dates must be nonnull and no later than maturity or redemption.
  Exercises cannot follow either date. These input guards are stronger than
  native QuantLib, which can construct unusable preceding-payment schedules.
- Impossible settlement lags and payment/settlement adjustments beyond the
  supported Date range return errors rather than integer wrapping or panics.
  These stronger guards apply to the wrapper. Immutable `bond()` queries
  retain their existing inherited behavior.
- Callability prices are per 100. No coupons accrue, so clean and dirty
  exercise quotes, and clean and dirty settlement prices, coincide.
- Past and settlement-day exercises are omitted. Remaining directions, prices
  and dates stay aligned; supplied order resolves coincident calls and puts.
- `bond()` exposes immutable cash flows, settlement, notional and accrual queries.
  Use `Instrument::base_mut().set_pricing_engine` on the callable wrapper,
  not a separate bond cache.

## Hull-White tree valuation and lifecycle

Attach the existing `TreeCallableFixedRateBondEngine` over a `HullWhite` model,
with positive time steps and the same shared `Settings` as the bond. The fitted
lattice includes exercise and adjusted redemption dates. `npv()` is in currency
units at the curve reference date. `settlement_value()` discounts that value
forward to settlement; quotes normalize it per 100 of current notional.
Missing engines, unset/null evaluation dates, unlinked curves and nonfinite
pricing inputs return errors.

Repeated reads cache results. Engine, curve, quote and evaluation-date changes
invalidate the same base. Freeze pins cached nonexpired values until unfreeze;
the inherited expired branch still returns zero even while frozen. At redemption
settlement current notional is zero, so quotes are zero even when today's flow
is included and NPV/settlement value remain positive.

Unlike native silent-flag caching, this new wrapper checks the two non-notifying
cash-flow inclusion flags whenever its `Instrument` base is accessed. Changed
flags silently invalidate cached results before the inherited calculation's
cache check, without notifying downstream observers or clearing freeze. This
avoids stale expiry results after flag changes. Existing instrument behavior is
unchanged.

## Oracle and bounded scope

Fixtures regenerate with actual native **QuantLib 1.43** and compare nonflat
TARGET-calendar calls/puts, clean/dirty quotes, unsorted/tied events, adjusted
redemption, settlement filtering, negative rates and zero redemption. Native
1.43 has a filtered-direction indexing defect when expired exercises precede
remaining opposite-direction rights. That defect is not imported: aligned
argument and pricing regressions preserve the parent's correct filtering.

OAS, calibration, implied volatility, duration/convexity, soft calls, convertibles,
Black callable engines and model-generic tree dispatch remain outside this scope.
