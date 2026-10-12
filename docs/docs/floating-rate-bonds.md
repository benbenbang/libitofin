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
  row and is not accepted by this constructor. No placeholder in-arrears
  pricing path is introduced. Current binding availability is described below.

## C, Go and Python bindings

The existing Rust construction contract above remains unchanged. The new
floating binding builds on the [shared bond facade](bond-bindings.md): one lazy
bond owns the instrument, while retained typed coupon handles permit installing
or replacing its coupon pricer after construction. Construction does not require
historical fixings, a forwarding curve result or an optionlet pricer.

```python
from itofin import Settings
from itofin.cashflows import BlackIborCouponPricer
from itofin.indexes import Currency, IborIndex
from itofin.instruments import FloatingRateBond
from itofin.pricingengines import DiscountingBondEngine
from itofin.termstructures import ConstantOptionletVolatility, FlatForward, VolatilityType
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Period, Schedule

settings = Settings()
settings.set_evaluation_date(Date(5, 1, 2024))
calendar = Calendar.null_calendar()
dc = DayCounter.actual360()
forwarding = FlatForward(Date(5, 1, 2024), 0.035, dc)
discount = FlatForward(Date(5, 1, 2024), 0.025, dc)
index = IborIndex(
    "Example", Period(3, "Months"), 2, Currency.eur(), calendar,
    BusinessDayConvention.Unadjusted, False, dc, forwarding, settings,
)
schedule = Schedule(
    Date(15, 1, 2024), Date(15, 1, 2027), Frequency.Quarterly,
    calendar, BusinessDayConvention.Unadjusted,
)
bond = FloatingRateBond(2, 1000.0, schedule, index, dc, settings, caps=[None, 0.04])
volatility = ConstantOptionletVolatility(
    Date(5, 1, 2024), calendar, BusinessDayConvention.Unadjusted,
    0.2, dc, VolatilityType.ShiftedLognormal,
)
bond.set_coupon_pricer(BlackIborCouponPricer(volatility))
bond.set_engine(DiscountingBondEngine(discount, settings))
print(bond.clean_price(), bond.index().tenor())
```

Go exposes `Session.NewFloatingRateBond(FloatingRateBondConfig)`,
`FloatingRateBond.Index`, `FloatingRateBond.SetCouponPricer` and
`Session.NewBlackIborCouponPricer`. Run the tracked example with
`go run ./examples/floating-bonds` from `sdk/go`, after building the C library.
The corresponding C constructors and inspectors use context-owned opaque
handles and additive configurations, without changing the basic bond layouts.

### Controls and dependency rules

- Payment convention, optional fixing-days override, gearings, spreads,
  per-entry caps/floors, redemption, issue date, ex-coupon controls and fixing
  convention are exposed. Optional Python controls are keyword-only.
- `fixing_days=None` uses the index lag; explicit zero remains zero. Empty
  gearings/spreads default to one/zero. Short vectors repeat their last entry;
  oversized vectors fail. Python caps/floors use `None` for an absent entry,
  and Go uses nil pointers. C uses the existing promoted `f32::MAX` null sentinel.
- Empty caps/floors receive the existing plain-coupon pricer. Nonempty vectors,
  even all-absent vectors, require explicit pricer installation. A zero-gearing
  coupon is fixed, pays the floored-then-capped spread and needs no fixing/pricer.
- A no-volatility pricer supports plain and known-fixing intrinsic optionlets.
  Only forecast cap/floor optionlets need a volatility surface. A fixing
  forecast on the evaluation date still uses an intrinsic optionlet.
- Floating ex-coupon period zero means absent, matching this Rust constructor.
  In-arrears convexity remains unsupported and is not a constructor option.
- The bond, index and engine must share the same settings allocation. Known
  moving constant optionlet surfaces also retain and check their settings and
  reference-date calendar metadata. Fixed/stripped surfaces carry no additional
  inferred settings identity; existing surface APIs remain unchanged.
- Known holiday-table bounds and the MOEX first year remain attached to
  calendars returned by the floating index and to joint calendars. Unsupported
  business-day queries return typed errors. Pure unadjusted date arithmetic
  does not query irrelevant holiday tables.
- Construction checks stored coupon/payment/fixing date inputs, not deferred
  forecast value/maturity dependencies. A known historical fixing can therefore
  price without querying an irrelevant forecast calendar; actual forecasts still
  validate the required date geometry and return typed boundary errors.
- Leg and individual cash-flow views retain index/pricer dependencies after
  original wrappers disappear, and see later pricer replacements. The retained
  Rust coupon observation chain invalidates the bond on replacement; this is
  stronger than native replacement-notification behavior, not a new cache model. Individual
  amounts are live coupon values, not frozen bond prices. Checked date guards
  apply only to the queried coupon and actual forecast-optionlet dependencies;
  irrelevant moving surfaces do not block plain, fixed or redemption amounts.
- Whole-leg pricing observes its actual expiry, inclusion and ex-coupon filters.
  Cached/frozen currency valuations do not preflight unused surfaces. Clean
  price and accrued amount still query the current accruing coupon, even while
  frozen, and guard its actual dependencies. Recalculation does preflight newly
  queried dependencies. Silent at-par/indexed policy changes
  invalidate live bond pricing while preserving the native freeze/expiry rules.

The public facades require positive finite face/redemption and finite coupon
parameters/results, with checked index/calendar/date boundaries. This does not
change the more permissive Rust constructor's monetary contract. Twelve tracked
QuantLib 1.43 scenarios cover plain/varying/mixed/negative/zero gearing,
Black/normal optionlets, explicit zero lag, stubs and ex-coupon coupons. Additional
lifecycle and boundary tests cover late fixings/pricers, retained old views,
pricer replacement, actual queried dependencies and freeze behavior.
