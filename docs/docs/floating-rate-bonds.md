# Floating-rate bonds

`instruments::FloatingRateBond` is an additive Rust-core instrument built from
`IborLeg` coupons and a single redemption. It composes the existing `Bond`:
use `bond()` for settlement/accrual and `bond_mut()` for engines and lazy prices.

## Supported contract

- Schedule calendar for settlement and payment adjustment.
- Explicit accrual day counter, optional fixing-days override and fixing convention.
- Per-coupon gearings/spreads/caps/floors. Shorter vectors repeat their last value;
  empty gearings/spreads mean 1/0 and empty caps/floors mean no option.
  `Null::null()` retains QuantLib's `Null<Real>` cap/floor sentinel per entry
  (`f32::MAX` promoted to `f64`), using `utilities::null::Null`; it
  does not acquire sentinel meaning for any other numeric parameter.
- Short and long stub reference periods, historical fixings and existing at-par
  or indexed forecast convention from the shared `Settings`.
- Negative gearing and zero gearing. Zero gearing creates a fixed coupon paying
  its spread, first floored then capped, with no index fixing or pricer needed.
- Ex-coupon calendar, convention, end-of-month flag and period. A zero-length
  period is treated as absent. Ex-coupon flows are excluded from NPV and accrued
  interest becomes negative through the existing coupon accrual rules.
- Redemption expressed per 100 of face amount; maturity is the schedule end,
  which can differ from the adjusted final payment date.
- Optional issue date. Like the original derived-bond constructor, late issue
  dates are accepted and settlement is clamped to issue, rather than applying
  the existing coupon-based `Bond::new` issue-date restriction.

## Pricing and observation

Empty cap/floor vectors receive the existing default Black ibor pricer. For
nonempty vectors, including vectors containing only null sentinels, call
`set_coupon_pricer` before pricing. This accepts the
existing `FloatingRateCouponPricer` interface; forecast optionlets need a
volatility-carrying pricer. `BlackIborCouponPricer::with_vol` supplies that path.
Known fixings use intrinsic optionlet values.

`ibor_coupons()` exposes underlying ibor handles in schedule order, with `None`
for fixed entries. Read full capped/floored rates and amounts through the base
cash flows. The index, coupon pricer, volatility, forwarding/discount curves
and evaluation date propagate invalidation to the existing lazy bond engine.
`into_bond()` retains cash flows and index observation, including all-fixed legs.
Attach the existing `DiscountingBondEngine` through
`bond_mut().base_mut().set_pricing_engine(...)` with the `Instrument` trait in scope.

## Safety, evidence and boundaries

- Bond and index must use the same `Settings` allocation. Use that allocation
  for the engine too. This is explicit rather than QuantLib's global singleton.
- Nonfinite numeric inputs, nonfinite scaled redemption, schedules with fewer
  than two dates and null schedule dates return errors. These are stronger
  construction safeguards than the original C++ finite-input contract.
- Ten native QuantLib 1.43 scenarios pin geometry, coupon amounts, collars,
  ex-coupon accrual, settlement, maturity and prices. The independent wheel is
  not the pinned original source revision; fixture provenance discloses both.
- In-arrears convexity adjustment is explicitly outside the indexed adoption
  row and is not accepted by this constructor. No placeholder pricing path,
  C/Go/Python facade or release is introduced.
