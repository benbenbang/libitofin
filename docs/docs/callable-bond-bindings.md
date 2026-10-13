# Callable bond bindings

Go and Python construct the existing hard-call/put callable fixed-rate and
zero-coupon products. These additive bindings are not marked released.
Both retain the actual callable instrument and its single lazy `Bond` base.
There is no separate cash-flow bond or second valuation cache.

## Python

```python
from itofin import Settings
from itofin.instruments import Callability, CallabilityType, CallableFixedRateBond, CallableZeroCouponBond
from itofin.termstructures import BondPriceType
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Schedule

settings = Settings()
settings.set_evaluation_date(Date(15, 3, 2026))
calendar = Calendar.null_calendar()
day_counter = DayCounter.actual360()
schedule = Schedule(
    Date(15, 1, 2026), Date(15, 1, 2030), Frequency.Annual,
    calendar, BusinessDayConvention.Unadjusted,
)
rights = [Callability(100.0, CallabilityType.Call, Date(15, 1, 2028), BondPriceType.Clean)]
fixed = CallableFixedRateBond(2, 250.0, schedule, [0.045], day_counter, rights, settings)
zero = CallableZeroCouponBond(2, calendar, 250.0, Date(15, 1, 2030), day_counter, rights, settings)
print(fixed.settlement_date(), fixed.accrued_amount(), zero.day_counter())
```

- `Callability` is immutable. `price()`, `price_type()`, `call_type()` and `date()`
  return its checked native values. Quote type defaults to Clean; price is per 100 of face.
- `callability()` returns a copied schedule snapshot. Rights retain their supplied
  order, including past, coincident, mixed call/put and unsorted events. Empty
  schedules are valid. No constructor sorting or deduplication occurs.
- Both callable classes inherit the existing nonconstructible `Bond` interface,
  not the concrete straight-bond classes. Fixed adds `frequency()` and
  `day_counter()`; zero adds its explicitly supplied payment `day_counter()`.
  Construction retains native frequency. A supported product with Once or
  NoFrequency still constructs; the restricted common frequency getter reports
  an unsupported-frequency error only when called.
- Python objects and their native dependencies remain thread-confined.

## Go and C

Go constructs validated immutable-value records with
`NewCallability(price, priceType, callType, date)`. `CallabilitySchedule` is a
slice of records whose fields are `Price`, `PriceType`, `Type` and `Date`.
Direction constants are `CallabilityCall` and `CallabilityPut`, avoiding the
existing option `Call`/`Put` names.

- `Session.NewCallableFixedRateBond(CallableFixedRateBondConfig)` and
  `Session.NewCallableZeroCouponBond(CallableZeroCouponBondConfig)` retain the
  actual native callable owners. Both embed the existing `Bond` interface.
- The fixed config embeds `FixedRateBondConfig` and adds `Callability`.
  The zero config embeds `ZeroCouponBondConfig` and adds `DayCounter` and
  `Callability`. Dependencies must belong to the same live session.
- `Callability()` returns a copied value slice; fixed exposes `Frequency()` and
  `DayCounter()`, while zero exposes `DayCounter()`.
- C uses the by-value `ItofinCallability` record, checked by
  `itofin_callability_new`. Callable constructors revalidate every supplied
  record, including records created directly by C/Go callers.
- `itofin_callable_bond_callability` uses a count and caller-owned output buffer.
  Normal registry, pointer, handle, session and thread contracts apply.

## Supported constructor controls

| Control | Callable fixed | Callable zero |
| --- | --- | --- |
| Calendar | Retained schedule calendar | Explicit calendar |
| Day counter | Accrual day counter | Required payment day counter |
| Payment convention | Following default | Following default |
| Redemption per 100 | 100 default, zero supported | 100 default, zero supported |
| Issue date | Optional | Optional |
| Ex-coupon controls | Period, calendar, convention, end-of-month | Not applicable |
| Payment-calendar override | Not supported | Main calendar is explicit |
| First-period day-counter override | Not supported | Not applicable |

Python omits unsupported fixed parameters. Go/C reject explicitly supplied fixed
payment-calendar or first-period-day-counter overrides rather than ignoring them.
Face is positive and finite. Redemption and exercise prices are finite and
nonnegative; generated/scaled cash-flow amounts must stay finite. Existing
straight-bond positive-redemption policy is unchanged.

Fixed ex-coupon convention defaults to Unadjusted and end-of-month to false;
an omitted ex-coupon calendar is NullCalendar, including when a period is supplied.
Negative ex-coupon periods are rejected. Explicit zero-length ex-coupon
period retains native payment-date semantics. Clean exercise prices include
indenture accrued interest independently of market ex-coupon accrued interest.

Exercise dates must be nonnull and no later than contractual maturity. Zero also
rejects rights after adjusted redemption. Checked calendar metadata and bounded
settlement/payment/ex-coupon traversals reject unsupported holiday-table dates
and date overflow. Unadjusted mathematical paths do not require unused holiday
lookups. Issue dates floor settlement and remain subject to native validation.

## Inherited analytics and lifecycle

The inherited [yield/curve/Z-spread methods](bond-analytics.md) value remaining
**contractual straight cash flows, without exercise optionality**. They are not
option-adjusted callable quotes or OAS. They read the same immutable bond and do
not install an engine, refresh flags, unfreeze, calculate or change the cache.

`npv`, settlement value, clean/dirty quotes, expiry, freeze/unfreeze and recalculate
dispatch through the actual callable native instrument. An unexpired bond without
a pricing engine reports an error. Native expired instruments return zero, even
when frozen. Freeze before an initial calculation does not fabricate results.
Callable zero silently refreshes inclusion-policy flags through its actual native
base, preserving freeze. Existing plain/floating and fixed policy behavior remains
unchanged.

The generic `DiscountingBondEngine` is the wrong engine contract for either
callable product. `Bond.set_engine` / `Bond.SetEngine` reject it before mutating
attachment or cache state. Callable engine bindings are a separate feature.

## Scope and evidence

Constructor, inherited analytics, ownership, calendar/date boundaries, wrong-engine
failure and native lazy-dispatch regressions cover this foundation. Existing
Rust/native callable numerical fixtures remain intact; this feature does not
claim new engine-bound numerical coverage. No core algorithm or public Rust API
changes are made.

Soft calls, convertibles, generic/G2 trees, full callable volatility surfaces and
multi-exercise Black approximations remain outside the approved binding scope.
